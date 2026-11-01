//! The factory (#18): the swarm at one moment *t*, as a factory building the app, read left to
//! right. Every ticket is a crate in its own colour, and at *t* it is in one place:
//!
//! - **up next**: its first event comes after *t* (the next few, with how long until);
//! - **the shelf**: it was worked on, but nobody holds it now (released, or left stopped);
//! - **a bench**: claimed, worked on, sent back or stopped, at the bench of the worker that
//!   holds it (the last to claim, work on or submit it by *t*);
//! - **the dock**: submitted, waiting for the merge, in the order they came;
//! - **the gate**: waiting, and a gate run going at *t* names its PR (or issue);
//! - **shipped**: merged (a block of the app on main), or done without a merge.
//!
//! Each bench has its actor (the worker's name, model and effort, on shift, paused or off) and a
//! helper's lamp (a `ready` check running, or just passed or failed). The gate has its
//! inspectors (the coordinators on shift), the runs going on, and the runs before *t*, passed or
//! failed with their cause (`rc=…; cause=…`). A merge whose gate left no `sub.start` counts as
//! a run of its own, at the merge. What happened in the `recent` seconds up to *t* is listed,
//! newest first, so landings and failures are seen, not inferred. Nothing after *t* is told:
//! a run going at *t* has no end yet, a crate's tooltip names only what had happened.
//!
//! Pure: the scene of the same events at the same *t* is the same.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{Issues, Kind, MainMerge, Role, Timestamp};

use crate::activity::{Activity, Agent, IssueEvent, Span, Sub, stop_reason};
use crate::colours::{NO_TICKET, Rgb, ticket_colour};
use crate::rows::duration;
use crate::tickets::{Phase, Stretch, Ticket, tickets};

/// The seconds before *t* whose events count as just happened, unless asked otherwise.
pub const DEFAULT_RECENT: i64 = 600;
/// The longest `recent` taken: a week.
pub const MAX_RECENT: i64 = 7 * 86_400;
/// A helper that never logged its stop is taken as running for this long, then as unknown.
pub const STALE_HELPER: i64 = 3_600;
/// How many tickets up next are listed, gate runs before *t* kept, and happenings listed.
pub const UPCOMING: usize = 8;
pub const GATE_HISTORY: usize = 8;
pub const HAPPENINGS: usize = 16;
/// The most crates listed at a station, and blocks shipped, before the rest are only counted.
pub const MAX_LISTED: usize = 64;
pub const MAX_BLOCKS: usize = 512;

/// Where a crate is, and what it is doing there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CrateState {
    /// Its first event comes after *t*.
    Upcoming,
    Claimed,
    Working,
    SentBack,
    Stopped,
    /// Submitted, waiting for the merge.
    Waiting,
    /// Waiting, and in a gate run going on.
    Gating,
    Merged,
    /// Finished without a merge (a `stop done` while worked on).
    Done,
    /// Worked on before, nobody holds it now.
    Shelved,
}

impl CrateState {
    /// How a person names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Upcoming => "up next",
            Self::Claimed => "claimed",
            Self::Working => "working",
            Self::SentBack => "sent back",
            Self::Stopped => "stopped",
            Self::Waiting => "waiting for merge",
            Self::Gating => "in the gate",
            Self::Merged => "merged",
            Self::Done => "done without a merge",
            Self::Shelved => "on the shelf",
        }
    }

    const fn of(phase: Phase) -> Self {
        match phase {
            Phase::Claimed => Self::Claimed,
            Phase::Working => Self::Working,
            Phase::Waiting => Self::Waiting,
            Phase::SentBack => Self::SentBack,
            Phase::Stopped => Self::Stopped,
        }
    }
}

/// A ticket at *t*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Crate {
    pub issue: u64,
    /// The pull request its events had named by *t*.
    pub pr: Option<u64>,
    /// `#<issue>`.
    pub label: String,
    /// Its title on GitHub, when the issues fetched have it ([`titled`]).
    pub title: Option<String>,
    /// Its own colour, the same in every scene and on the timeline: [`ticket_colour`].
    pub colour: Rgb,
    pub state: CrateState,
    /// When it came into this state; up next, when its first event comes.
    pub since: Timestamp,
    /// Seconds in this state at *t*; up next, seconds until.
    pub age: i64,
    /// The name of the worker that holds it, or held it last.
    pub worker: Option<String>,
    /// The note of the event that put it here (a send-back's cause, a stop's reason).
    pub note: Option<String>,
    pub tooltip: Vec<String>,
}

/// Whether an actor is at work at *t*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Shift {
    /// A session of its is going on.
    On,
    /// Its session is paused.
    Paused,
    /// No session of its is going on.
    Off,
}

/// A worker or a coordinator: every instance with its machine and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub machine: String,
    pub name: String,
    /// The model and effort its latest session by *t* named.
    pub model: Option<String>,
    pub effort: Option<String>,
    pub shift: Shift,
}

/// How a gate run or a helper's check went, at *t*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Running,
    /// `rc=0`, or the run merged a ticket.
    Passed,
    /// A non-zero `rc`, or the run only sent tickets back.
    Failed,
    /// It ended without saying.
    Unknown,
}

/// A helper sub-agent of a worker's (a `ready` check before submitting), running at *t* or
/// just finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Helper {
    pub sub: String,
    /// Its name (`ready`), else its id.
    pub name: String,
    pub start: Timestamp,
    pub outcome: Outcome,
    /// Seconds since it started (running) or ended.
    pub age: i64,
    pub tooltip: Vec<String>,
}

/// What a bench is doing at *t*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BenchStatus {
    Working,
    Claimed,
    SentBack,
    Stopped,
    /// On shift, holding nothing to work on.
    Idle,
    Paused,
    Off,
}

impl BenchStatus {
    /// How a person names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Claimed => "claimed",
            Self::SentBack => "sent back",
            Self::Stopped => "stopped",
            Self::Idle => "idle",
            Self::Paused => "paused",
            Self::Off => "off shift",
        }
    }
}

/// A worker's bench.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Bench {
    /// `<machine>/<name>`: stable across scenes.
    pub key: String,
    pub actor: Actor,
    pub status: BenchStatus,
    /// The crates it holds, the one it began on last first.
    pub crates: Vec<Crate>,
    pub helper: Option<Helper>,
    pub tooltip: Vec<String>,
}

/// A coordinator's gate run: a `sub.start` / `sub.stop` of a gate, or the merges and send-backs
/// of a gate that logged neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GateRun {
    /// The sub-agent's id: `gate-116`.
    pub sub: String,
    pub start: Timestamp,
    /// When it ended; none while it runs at *t*.
    pub end: Option<Timestamp>,
    pub outcome: Outcome,
    /// The stop note's `cause=`, or a send-back's.
    pub cause: Option<String>,
    /// Its `kind=`: smoke, full, docs.
    pub kind: Option<String>,
    /// The pull requests and issues its start note names (`#<m>`), and the issues its events name.
    pub refs: Vec<u64>,
    pub issues: Vec<u64>,
    /// Read from its merges and send-backs: no `sub.start` was logged.
    pub inferred: bool,
    /// Seconds since it started (running) or ended.
    pub age: i64,
    pub tooltip: Vec<String>,
}

/// The gate: the coordinators, what is being gated, and how the runs before *t* went.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Gate {
    pub inspectors: Vec<Actor>,
    pub running: Vec<GateRun>,
    /// The waiting crates a run going on names.
    pub crates: Vec<Crate>,
    /// The runs that ended by *t*, the last first.
    pub recent: Vec<GateRun>,
    pub passed: usize,
    pub failed: usize,
}

/// A merge onto main by *t*: a block of the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub t: Timestamp,
    pub pr: Option<u64>,
    /// The ticket whose merge names its PR.
    pub issue: Option<u64>,
    /// `PR #<m>`, else the commit's start.
    pub label: String,
    /// What its brick says: `#<issue>` for a ticket's merge; `!<m>`, its PR, for a merge no
    /// ticket names (setup, docs); else the commit's start.
    pub mark: String,
    /// Its ticket's colour; [`NO_TICKET`] without one, so it reads as not a ticket.
    pub colour: Rgb,
    pub age: i64,
    pub tooltip: Vec<String>,
}

/// Shipping: the app on main, block by block, and the tickets done without a merge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Shipping {
    /// The merges onto main by *t*, oldest first (the last [`MAX_BLOCKS`]); the tickets'
    /// merges when the repository has no main.
    pub blocks: Vec<Block>,
    /// How many there are by *t*, and in all.
    pub landed: usize,
    pub total: usize,
    pub merged_tickets: usize,
    /// Done without a merge, the last first.
    pub done: Vec<Crate>,
    pub done_count: usize,
}

/// What a happening was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum HappeningKind {
    Claimed,
    Began,
    Submitted,
    SentBack,
    Stopped,
    Released,
    Merged,
    Done,
    /// A merge onto main.
    Landed,
    GatePassed,
    GateFailed,
}

/// Something that happened in the `recent` seconds up to *t*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Happening {
    pub t: Timestamp,
    pub age: i64,
    pub kind: HappeningKind,
    pub issue: Option<u64>,
    /// The ticket's colour, for one.
    pub colour: Option<Rgb>,
    pub text: String,
}

/// How many crates are where.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub upcoming: usize,
    pub shelved: usize,
    pub at_benches: usize,
    pub waiting: usize,
    pub gating: usize,
    pub merged: usize,
    pub done: usize,
}

/// The factory at *t*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FactoryScene {
    pub t: Timestamp,
    /// *t* as `YYYY-MM-DDTHH:MM:SSZ`.
    pub text: String,
    /// The seconds before *t* that count as just happened.
    pub recent: i64,
    /// The first and last event's times.
    pub first: Option<Timestamp>,
    pub last: Option<Timestamp>,
    /// The next tickets, soonest first (the next [`UPCOMING`]).
    pub upcoming: Vec<Crate>,
    pub shelf: Vec<Crate>,
    pub benches: Vec<Bench>,
    /// Waiting for the merge, longest first.
    pub dock: Vec<Crate>,
    pub gate: Gate,
    pub shipping: Shipping,
    /// The newest first.
    pub happenings: Vec<Happening>,
    pub counts: Counts,
    /// The journey of the ticket asked for ([`journey`]), when one was.
    pub journey: Option<Journey>,
}

/// A station a ticket passes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Station {
    Bench,
    Dock,
    Gate,
    Main,
    /// Done without a merge.
    Done,
}

impl Station {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bench => "bench",
            Self::Dock => "dock",
            Self::Gate => "gate",
            Self::Main => "main",
            Self::Done => "done",
        }
    }
}

/// A stay at one station: from `start`, `seconds` long (by *t*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Leg {
    pub station: Station,
    pub start: Timestamp,
    pub seconds: i64,
    /// `bench 42m`; `main` for the end of the line.
    pub label: String,
}

/// A ticket's way through the stations by *t*: `bench 42m → dock 6m → gate 3m → main`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Journey {
    pub issue: u64,
    pub legs: Vec<Leg>,
    pub text: String,
}

/// A short duration, for a label: `45s`, `12m`, `3h05`, `2d04h`.
#[must_use]
pub fn short(seconds: i64) -> String {
    let s = seconds.max(0);
    match s {
        0..60 => format!("{s}s"),
        60..3_600 => format!("{}m", s / 60),
        3_600..86_400 => format!("{}h{:02}", s / 3_600, (s % 3_600) / 60),
        _ => format!("{}d{:02}h", s / 86_400, (s % 86_400) / 3_600),
    }
}

/// `field=<value>` from a note's `;`-separated fields, its first word.
#[must_use]
pub fn note_field(note: &str, field: &str) -> Option<String> {
    note.split(';').find_map(|part| {
        let value = part.trim().strip_prefix(field)?.strip_prefix('=')?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// Who each instance is: its machine and name.
struct Names<'a> {
    activity: &'a Activity,
}

impl Names<'_> {
    fn key(&self, instance: &str) -> String {
        self.activity.agent(instance).map_or_else(|| instance.to_owned(), |a| format!("{}/{}", a.machine, a.name))
    }

    fn name(&self, instance: &str) -> String {
        self.activity.agent(instance).map_or_else(|| instance.to_owned(), |a| a.name.clone())
    }
}

/// The factory of `activity` and the merges onto main at `t`, with `recent` seconds counting as
/// just happened (clamped to 1 s .. [`MAX_RECENT`]).
#[must_use]
pub fn scene(activity: &Activity, merges: &[MainMerge], t: Timestamp, recent: i64) -> FactoryScene {
    let recent = recent.clamp(1, MAX_RECENT);
    let names = Names { activity };
    let tickets = tickets(activity);
    let runs = gate_runs(activity, t);
    let running: Vec<&GateRun> = runs.iter().filter(|r| r.outcome == Outcome::Running).collect();
    let holders = holders(activity, t);
    let mut crates: Vec<Crate> = tickets.iter().map(|ticket| crate_at(ticket, t, holders.get(&ticket.issue).map(|i| names.name(i)))).collect();
    for c in &mut crates {
        if c.state == CrateState::Waiting && running.iter().any(|r| c.pr.is_some_and(|pr| r.refs.contains(&pr)) || r.issues.contains(&c.issue)) {
            c.state = CrateState::Gating;
            if let Some(line) = c.tooltip.get_mut(1) {
                *line = format!("in the gate: {}", running.iter().map(|r| r.sub.as_str()).collect::<Vec<_>>().join(", "));
            }
        }
    }
    let counts = count(&crates);
    let mut at_bench: BTreeMap<String, Vec<Crate>> = BTreeMap::new();
    let (mut upcoming, mut shelf, mut dock, mut gating, mut done) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for c in crates {
        match c.state {
            CrateState::Upcoming => upcoming.push(c),
            CrateState::Shelved => shelf.push(c),
            CrateState::Claimed | CrateState::Working | CrateState::SentBack | CrateState::Stopped => {
                let key = holders.get(&c.issue).map_or_else(|| "-/no worker".to_owned(), |i| names.key(i));
                at_bench.entry(key).or_default().push(c);
            }
            CrateState::Waiting => dock.push(c),
            CrateState::Gating => gating.push(c),
            CrateState::Done => done.push(c),
            CrateState::Merged => {}
        }
    }
    upcoming.sort_by_key(|c| (c.since, c.issue));
    upcoming.truncate(UPCOMING);
    shelf.sort_by_key(|c| (std::cmp::Reverse(c.since), c.issue));
    shelf.truncate(MAX_LISTED);
    dock.sort_by_key(|c| (c.since, c.issue));
    dock.truncate(MAX_LISTED);
    done.sort_by_key(|c| (std::cmp::Reverse(c.since), c.issue));
    let done_count = done.len();
    done.truncate(MAX_LISTED);
    let (first, last) = activity.extent.unzip();
    FactoryScene {
        t,
        text: t.to_string(),
        recent,
        first,
        last,
        upcoming,
        shelf,
        benches: benches(activity, t, recent, at_bench),
        dock,
        gate: gate(activity, t, &runs, gating),
        shipping: shipping(merges, &tickets, t, done, done_count),
        happenings: happenings(activity, merges, &runs, t, recent),
        counts,
        journey: None,
    }
}

/// `issue`'s journey by `t`: its stretches at a bench (claimed, working, sent back, stopped), at
/// the dock (waiting) less the time a gate run naming it went on (the gate), then main or done.
/// Consecutive stays at one station are one leg; none for a ticket with no events.
#[must_use]
pub fn journey(activity: &Activity, t: Timestamp, issue: u64) -> Option<Journey> {
    let ticket = tickets(activity).into_iter().find(|tk| tk.issue == issue)?;
    let runs = gate_runs(activity, t);
    let naming: Vec<(Timestamp, Timestamp)> =
        runs.iter().filter(|r| ticket.pr.is_some_and(|pr| r.refs.contains(&pr)) || r.issues.contains(&issue)).map(|r| (r.start, r.end.unwrap_or(t))).collect();
    let mut legs: Vec<Leg> = Vec::new();
    for stretch in ticket.stretches.iter().filter(|s| s.start <= t) {
        let end = if stretch.open { t } else { stretch.end.min(t) };
        if stretch.phase == Phase::Waiting {
            wait(&mut legs, (stretch.start, end), &naming);
        } else {
            stay(&mut legs, Station::Bench, stretch.start, end.0.saturating_sub(stretch.start.0));
        }
    }
    legs.retain(|leg| leg.seconds > 0);
    let last = match (&ticket.merged, &ticket.done) {
        (Some(merged), _) if merged.t <= t => Some((Station::Main, merged.t)),
        (_, Some(done)) if done.t <= t && ticket.merged.is_none() => Some((Station::Done, done.t)),
        _ => None,
    };
    if let Some((station, at)) = last {
        legs.push(Leg { station, start: at, seconds: 0, label: station.name().to_owned() });
    }
    for leg in legs.iter_mut().filter(|l| !matches!(l.station, Station::Main | Station::Done)) {
        leg.label = format!("{} {}", leg.station.name(), short(leg.seconds));
    }
    let text = legs.iter().map(|l| l.label.as_str()).collect::<Vec<_>>().join(" → ");
    Some(Journey { issue, legs, text })
}

/// A wait for the merge, `span`, in time order: at the dock until a gate run naming the ticket
/// (`runs`) starts, in the gate while it goes.
fn wait(legs: &mut Vec<Leg>, span: (Timestamp, Timestamp), runs: &[(Timestamp, Timestamp)]) {
    let (start, end) = span;
    let mut runs: Vec<(Timestamp, Timestamp)> = runs.iter().map(|(a, b)| ((*a).max(start), (*b).min(end))).filter(|(a, b)| a < b).collect();
    runs.sort();
    let mut at = start;
    for (from, to) in runs {
        stay(legs, Station::Dock, at, from.0.saturating_sub(at.0));
        stay(legs, Station::Gate, from.max(at), to.0.saturating_sub(from.max(at).0));
        at = at.max(to);
    }
    stay(legs, Station::Dock, at, end.0.saturating_sub(at.0));
}

/// Adds `seconds` at `station`: to the last leg when it is there already.
fn stay(legs: &mut Vec<Leg>, station: Station, start: Timestamp, seconds: i64) {
    if seconds <= 0 {
        return;
    }
    match legs.last_mut() {
        Some(last) if last.station == station => last.seconds += seconds,
        _ => legs.push(Leg { station, start, seconds, label: String::new() }),
    }
}

fn count(crates: &[Crate]) -> Counts {
    let n = |states: &[CrateState]| crates.iter().filter(|c| states.contains(&c.state)).count();
    Counts {
        upcoming: n(&[CrateState::Upcoming]),
        shelved: n(&[CrateState::Shelved]),
        at_benches: n(&[CrateState::Claimed, CrateState::Working, CrateState::SentBack, CrateState::Stopped]),
        waiting: n(&[CrateState::Waiting]),
        gating: n(&[CrateState::Gating]),
        merged: n(&[CrateState::Merged]),
        done: n(&[CrateState::Done]),
    }
}

/// The instance holding each ticket at `t`: the last to claim, work on or submit it, not a gate.
fn holders(activity: &Activity, t: Timestamp) -> BTreeMap<u64, String> {
    let mut holders = BTreeMap::new();
    for event in activity.issue_events.iter().take_while(|e| e.t <= t) {
        if event.sub.is_none() && matches!(event.kind, Kind::Claim { .. } | Kind::Work { .. } | Kind::Submit { .. }) {
            holders.insert(event.issue, event.instance.clone());
        }
    }
    holders
}

/// A stretch going on at `t`: begun by then, and not ended (or nothing ended it).
fn covers(stretch: &Stretch, t: Timestamp) -> bool {
    stretch.start <= t && (t < stretch.end || stretch.open)
}

/// Where `ticket` is at `t`, and since when.
fn place(ticket: &Ticket, t: Timestamp) -> (CrateState, Timestamp, Option<String>) {
    if t < ticket.first {
        return (CrateState::Upcoming, ticket.first, None);
    }
    if let Some(stretch) = ticket.stretches.iter().rev().find(|s| covers(s, t)) {
        return (CrateState::of(stretch.phase), stretch.start, stretch.note.clone());
    }
    if let Some(merged) = ticket.merged.as_ref().filter(|m| m.t <= t) {
        return (CrateState::Merged, merged.t, None);
    }
    if let Some(done) = ticket.done.as_ref().filter(|d| d.t <= t) {
        return (CrateState::Done, done.t, done.note.clone());
    }
    let since = ticket.stretches.iter().map(|s| s.end).filter(|end| *end <= t).max().unwrap_or(ticket.first);
    (CrateState::Shelved, since, None)
}

fn crate_at(ticket: &Ticket, t: Timestamp, worker: Option<String>) -> Crate {
    let (state, since, note) = place(ticket, t);
    let age = if state == CrateState::Upcoming { since.0.saturating_sub(t.0) } else { t.0.saturating_sub(since.0) };
    // The PR it had named by t: its merge's, else its last event's by then.
    let pr = ticket.merged.as_ref().filter(|m| m.t <= t).and_then(|m| m.pr).or_else(|| ticket.pr.filter(|_| ticket.claimed.is_some_and(|c| c <= t)));
    let title = pr.map_or_else(|| format!("#{}", ticket.issue), |pr| format!("#{} · PR #{pr}", ticket.issue));
    let mut tooltip = vec![title];
    tooltip.push(match state {
        CrateState::Upcoming => format!("up next: its first event in {}", duration(age)),
        _ => format!("{} for {} (since {since})", state.name(), duration(age)),
    });
    if let Some(worker) = &worker {
        tooltip.push(format!("worker: {worker}"));
    }
    tooltip.extend(note.iter().cloned());
    let mut milestone = |what: &str, at: Option<Timestamp>| {
        if let Some(at) = at.filter(|at| *at <= t) {
            tooltip.push(format!("{what:<9} {at}"));
        }
    };
    milestone("claimed", ticket.claimed);
    milestone("began", ticket.began);
    milestone("finished", ticket.finished);
    milestone("merged", ticket.merged.as_ref().map(|m| m.t));
    Crate {
        issue: ticket.issue,
        pr,
        label: format!("#{}", ticket.issue),
        title: None,
        colour: ticket_colour(ticket.issue),
        state,
        since,
        age,
        worker,
        note,
        tooltip,
    }
}

/// The scene with what GitHub's `issues` say of its tickets (#17): each crate GitHub has carries
/// its title, and its tooltip starts with the title, its state and labels as of the scene's *t*,
/// and its pull request ([`crate::titles::header`]) in place of `#170 · PR #173`; a ticket's
/// block on main, and a merge's whose PR GitHub has, start with the title too.
pub fn titled(scene: &mut FactoryScene, issues: &Issues) {
    if issues.is_empty() {
        return;
    }
    let t = scene.t;
    let bench_crates = scene.benches.iter_mut().flat_map(|b| b.crates.iter_mut());
    let all = scene.upcoming.iter_mut().chain(scene.shelf.iter_mut()).chain(bench_crates).chain(scene.dock.iter_mut());
    for c in all.chain(scene.gate.crates.iter_mut()).chain(scene.shipping.done.iter_mut()) {
        let Some(header) = crate::titles::header(issues, c.issue, c.pr, Some(t)) else { continue };
        c.title = crate::titles::title(issues, c.issue).map(str::to_owned);
        if !c.tooltip.is_empty() {
            c.tooltip.splice(0..1, header);
        }
    }
    for block in &mut scene.shipping.blocks {
        let title = match (block.issue, block.pr) {
            (Some(issue), _) => {
                crate::titles::title(issues, issue).map(|title| format!("#{issue} {}", crate::titles::short(title, crate::titles::TOOLTIP_TITLE)))
            }
            (None, Some(pr)) => crate::titles::title(issues, pr).map(|title| format!("PR #{pr} {}", crate::titles::short(title, crate::titles::TOOLTIP_TITLE))),
            (None, None) => None,
        };
        if let Some(title) = title {
            block.tooltip.insert(0, title);
        }
    }
}

fn in_span(span: &Span, t: Timestamp) -> bool {
    span.start <= t && (t < span.end || span.open)
}

/// The instances of each actor (machine and name), by key.
fn actors(activity: &Activity) -> BTreeMap<String, Vec<&Agent>> {
    let mut actors: BTreeMap<String, Vec<&Agent>> = BTreeMap::new();
    for agent in &activity.agents {
        actors.entry(format!("{}/{}", agent.machine, agent.name)).or_default().push(agent);
    }
    actors
}

fn actor(instances: &[&Agent], t: Timestamp) -> Actor {
    let latest = instances.iter().filter(|a| a.sessions.first().is_some_and(|s| s.span.start <= t)).max_by_key(|a| a.sessions.first().map(|s| s.span.start));
    let shown = latest.or_else(|| instances.first()).copied();
    let sessions = || instances.iter().flat_map(|a| &a.sessions);
    let shift = match sessions().find(|s| in_span(&s.span, t)) {
        Some(session) if session.pauses.iter().any(|p| in_span(p, t)) => Shift::Paused,
        Some(_) => Shift::On,
        None => Shift::Off,
    };
    Actor {
        machine: shown.map(|a| a.machine.clone()).unwrap_or_default(),
        name: shown.map(|a| a.name.clone()).unwrap_or_default(),
        model: shown.and_then(|a| a.model.clone()),
        effort: shown.and_then(|a| a.effort.clone()),
        shift,
    }
}

fn is_coordinator(instances: &[&Agent]) -> bool {
    instances.iter().all(|a| a.role == Role::Coordinator)
}

/// A bench per worker (every actor not only a coordinator), and one for any other holding a
/// crate at `t`.
fn benches(activity: &Activity, t: Timestamp, recent: i64, mut held: BTreeMap<String, Vec<Crate>>) -> Vec<Bench> {
    let mut benches: Vec<Bench> = Vec::new();
    for (key, instances) in actors(activity) {
        let crates = held.remove(&key).unwrap_or_default();
        if is_coordinator(&instances) && crates.is_empty() {
            continue;
        }
        benches.push(bench(key, actor(&instances, t), crates, helper(&instances, t, recent)));
    }
    for (key, crates) in held {
        let actor = Actor { machine: String::new(), name: key.clone(), model: None, effort: None, shift: Shift::Off };
        benches.push(bench(key, actor, crates, None));
    }
    benches
}

fn bench(key: String, actor: Actor, mut crates: Vec<Crate>, helper: Option<Helper>) -> Bench {
    // The crate it began on last first; one being worked on before one merely held.
    let rank = |c: &Crate| match c.state {
        CrateState::Working => 0,
        CrateState::SentBack => 1,
        CrateState::Claimed => 2,
        _ => 3,
    };
    crates.sort_by_key(|c| (rank(c), std::cmp::Reverse(c.since), c.issue));
    let status = match (actor.shift, crates.first().map(|c| c.state)) {
        (_, Some(CrateState::Working)) => BenchStatus::Working,
        (_, Some(CrateState::SentBack)) => BenchStatus::SentBack,
        (_, Some(CrateState::Claimed)) => BenchStatus::Claimed,
        (Shift::Off, _) => BenchStatus::Off,
        (Shift::Paused, _) => BenchStatus::Paused,
        (Shift::On, Some(CrateState::Stopped)) => BenchStatus::Stopped,
        (Shift::On, _) => BenchStatus::Idle,
    };
    let mut tooltip = vec![actor.name.clone()];
    if !actor.machine.is_empty() {
        tooltip.push(format!("on {}", actor.machine));
    }
    if let Some(model) = &actor.model {
        tooltip.push(actor.effort.as_ref().map_or_else(|| model.clone(), |effort| format!("{model} · {effort} effort")));
    }
    tooltip.push(match crates.first() {
        Some(c) => format!("{}: {} for {}", status.name(), c.label, duration(c.age)),
        None => status.name().to_owned(),
    });
    if crates.len() > 1 {
        tooltip.push(format!("holds {}", crates.iter().map(|c| format!("{} ({})", c.label, c.state.name())).collect::<Vec<_>>().join(", ")));
    }
    Bench { key, actor, status, crates, helper, tooltip }
}

/// A worker's helper (not a gate) running at `t`, else the one that ended last in `recent`.
fn helper(instances: &[&Agent], t: Timestamp, recent: i64) -> Option<Helper> {
    // A helper runs only while its instance's session does: one that never logged its stop
    // doesn't run on after its worker went home, and has no end to tell.
    let live = |a: &Agent| a.sessions.iter().any(|s| in_span(&s.span, t));
    let subs = || instances.iter().flat_map(|a| a.subs.iter().map(move |s| (*a, s))).filter(|(_, s)| !s.is_gate() && s.span.start <= t);
    let running = subs().find(|(a, s)| (s.span.start < s.span.end || s.span.open) && in_span(&s.span, t) && live(a)).map(|(_, s)| s);
    let (sub, outcome) = match running {
        // Running past an hour with no stop logged ever: it's not known to be running.
        Some(sub) if sub.span.open && t.0.saturating_sub(sub.span.start.0) > STALE_HELPER => (sub, Outcome::Unknown),
        Some(sub) => (sub, Outcome::Running),
        None => {
            let finished = subs().map(|(_, s)| s).filter(|s| !s.span.open && s.span.end <= t && t.0.saturating_sub(s.span.end.0) <= recent);
            let sub = finished.max_by_key(|s| s.span.end)?;
            (sub, ended(sub.rc))
        }
    };
    let name = sub.name.clone().unwrap_or_else(|| sub.sub.clone());
    let age = t.0.saturating_sub(if running.is_some() { sub.span.start.0 } else { sub.span.end.0 });
    let mut tooltip = vec![format!("{name} ({})", sub.sub)];
    tooltip.extend(sub.start_note.iter().cloned());
    tooltip.push(match outcome {
        Outcome::Running => format!("running for {}", duration(age)),
        _ if running.is_some() => format!("started {} ago; no stop logged", duration(age)),
        _ => format!("{} {} ago: {}", outcome_name(outcome), duration(age), sub.stop_note.as_deref().unwrap_or("no note")),
    });
    Some(Helper { sub: sub.sub.clone(), name, start: sub.span.start, outcome, age, tooltip })
}

const fn ended(rc: Option<i64>) -> Outcome {
    match rc {
        Some(0) => Outcome::Passed,
        Some(_) => Outcome::Failed,
        None => Outcome::Unknown,
    }
}

const fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Running => "running",
        Outcome::Passed => "passed",
        Outcome::Failed => "failed",
        Outcome::Unknown => "ended",
    }
}

/// Every gate run begun by `t`, by start: the gate sub-agents, then a run for each gate the
/// merges and send-backs name that logged no `sub.start` or `sub.stop`.
fn gate_runs(activity: &Activity, t: Timestamp) -> Vec<GateRun> {
    let subs: Vec<&Sub> = activity.agents.iter().flat_map(|a| &a.subs).filter(|s| s.is_gate()).collect();
    let mut runs: Vec<GateRun> = subs.iter().filter(|s| s.span.start <= t).map(|s| logged_run(s, t)).collect();
    let mut inferred: BTreeMap<&str, Vec<&IssueEvent>> = BTreeMap::new();
    for event in activity.issue_events.iter().take_while(|e| e.t <= t) {
        if let Some(sub) = event.sub.as_deref().filter(|sub| sub.starts_with("gate") && !subs.iter().any(|s| s.sub == *sub))
            && matches!(event.kind, Kind::Merge { .. } | Kind::Sendback { .. })
        {
            inferred.entry(sub).or_default().push(event);
        }
    }
    runs.extend(inferred.into_iter().filter_map(|(sub, events)| inferred_run(sub, &events, t)));
    runs.sort_by(|a, b| (a.start, &a.sub).cmp(&(b.start, &b.sub)));
    runs
}

fn logged_run(sub: &Sub, t: Timestamp) -> GateRun {
    let running = (sub.span.open || sub.span.start < sub.span.end) && in_span(&sub.span, t);
    let outcome = if running { Outcome::Running } else { ended(sub.rc) };
    let end = (!running).then_some(sub.span.end);
    let notes = || sub.stop_note.iter().filter(|_| !running).chain(sub.start_note.iter());
    let cause = sub.stop_note.as_deref().filter(|_| !running).and_then(|n| note_field(n, "cause"));
    let kind = notes().find_map(|n| note_field(n, "kind")).and_then(|k| k.split_whitespace().next().map(str::to_owned));
    let age = t.0.saturating_sub(end.unwrap_or(sub.span.start).0);
    let mut tooltip = vec![format!("gate run {}", sub.sub)];
    tooltip.extend(sub.start_note.iter().cloned());
    tooltip.push(match end {
        None => format!("running for {} (since {})", duration(age), sub.span.start),
        Some(end) => format!("{}: {} → {end} ({})", outcome_name(outcome), sub.span.start, duration(end.0.saturating_sub(sub.span.start.0))),
    });
    if !running {
        tooltip.extend(sub.stop_note.iter().cloned());
    }
    GateRun {
        sub: sub.sub.clone(),
        start: sub.span.start,
        end,
        outcome,
        cause,
        kind,
        refs: sub.prs.clone(),
        issues: sub.issues.clone(),
        inferred: false,
        age,
        tooltip,
    }
}

fn inferred_run(sub: &str, events: &[&IssueEvent], t: Timestamp) -> Option<GateRun> {
    let (start, end) = (events.first()?.t, events.last()?.t);
    let merged = events.iter().any(|e| matches!(e.kind, Kind::Merge { .. }));
    let outcome = if merged { Outcome::Passed } else { Outcome::Failed };
    let cause = (!merged).then(|| events.iter().find_map(|e| e.note.as_deref().map(|n| note_field(n, "cause").unwrap_or_else(|| n.to_owned())))).flatten();
    let mut refs: Vec<u64> = events.iter().filter_map(|e| e.kind.mr()).collect();
    refs.dedup();
    let mut issues: Vec<u64> = events.iter().map(|e| e.issue).collect();
    issues.dedup();
    let what = |e: &IssueEvent| format!("{} #{}{}", e.kind.name(), e.issue, e.kind.mr().map(|m| format!(" (PR #{m})")).unwrap_or_default());
    let mut tooltip = vec![format!("gate run {sub}"), format!("{} at {end} (read from its events: no sub.start logged)", outcome_name(outcome))];
    tooltip.extend(events.iter().map(|e| what(e)));
    Some(GateRun {
        sub: sub.to_owned(),
        start,
        end: Some(end),
        outcome,
        cause,
        kind: None,
        refs,
        issues,
        inferred: true,
        age: t.0.saturating_sub(end.0),
        tooltip,
    })
}

fn gate(activity: &Activity, t: Timestamp, runs: &[GateRun], mut crates: Vec<Crate>) -> Gate {
    let inspectors = actors(activity).values().filter(|i| is_coordinator(i)).map(|i| actor(i, t)).collect();
    let running = runs.iter().filter(|r| r.outcome == Outcome::Running).cloned().collect();
    let mut ended: Vec<&GateRun> = runs.iter().filter(|r| r.outcome != Outcome::Running).collect();
    ended.sort_by_key(|r| std::cmp::Reverse((r.end, r.start)));
    let passed = ended.iter().filter(|r| r.outcome == Outcome::Passed).count();
    let failed = ended.iter().filter(|r| r.outcome == Outcome::Failed).count();
    crates.sort_by_key(|c| (c.since, c.issue));
    Gate { inspectors, running, crates, recent: ended.into_iter().take(GATE_HISTORY).cloned().collect(), passed, failed }
}

fn shipping(merges: &[MainMerge], tickets: &[Ticket], t: Timestamp, done: Vec<Crate>, done_count: usize) -> Shipping {
    let issue_of = |pr: u64| tickets.iter().find(|tk| tk.merged.as_ref().is_some_and(|m| m.pr == Some(pr))).map(|tk| tk.issue);
    let mut blocks: Vec<Block> = if merges.is_empty() {
        tickets.iter().filter_map(|tk| tk.merged.as_ref().map(|m| ticket_block(tk.issue, m.pr, m.t, t))).collect()
    } else {
        merges.iter().map(|m| main_block(m, m.pr.and_then(issue_of), t)).collect()
    };
    let total = blocks.len();
    blocks.retain(|b| b.t <= t);
    blocks.sort_by_key(|b| b.t);
    let landed = blocks.len();
    let skip = landed.saturating_sub(MAX_BLOCKS);
    let merged_tickets = tickets.iter().filter(|tk| tk.merged.as_ref().is_some_and(|m| m.t <= t)).count();
    Shipping { blocks: blocks.into_iter().skip(skip).collect(), landed, total, merged_tickets, done, done_count }
}

fn main_block(merge: &MainMerge, issue: Option<u64>, t: Timestamp) -> Block {
    let short_commit = merge.commit.get(..8).unwrap_or(&merge.commit);
    let label = merge.pr.map_or_else(|| short_commit.to_owned(), |pr| format!("PR #{pr}"));
    let mut tooltip = vec![merge.subject.clone(), format!("{short_commit} · {}", merge.t)];
    tooltip.push(issue.map_or_else(|| "no ticket's merge names it".to_owned(), |issue| format!("ticket #{issue}")));
    let mark = match (issue, merge.pr) {
        (Some(issue), _) => format!("#{issue}"),
        (None, Some(pr)) => format!("!{pr}"),
        (None, None) => short_commit.get(..7).unwrap_or(short_commit).to_owned(),
    };
    let colour = issue.map_or(NO_TICKET, ticket_colour);
    Block { t: merge.t, pr: merge.pr, issue, label, mark, colour, age: t.0.saturating_sub(merge.t.0), tooltip }
}

fn ticket_block(issue: u64, pr: Option<u64>, at: Timestamp, t: Timestamp) -> Block {
    let label = pr.map_or_else(|| format!("#{issue}"), |pr| format!("PR #{pr}"));
    let tooltip = vec![format!("#{issue} merged"), at.to_string()];
    Block { t: at, pr, issue: Some(issue), label, mark: format!("#{issue}"), colour: ticket_colour(issue), age: t.0.saturating_sub(at.0), tooltip }
}

/// What happened in the `recent` seconds up to `t`, newest first.
fn happenings(activity: &Activity, merges: &[MainMerge], runs: &[GateRun], t: Timestamp, recent: i64) -> Vec<Happening> {
    let names = Names { activity };
    let from = Timestamp(t.0.saturating_sub(recent));
    let fresh = |at: Timestamp| from < at && at <= t;
    let mut list: Vec<Happening> =
        activity.issue_events.iter().filter(|e| fresh(e.t)).filter_map(|e| issue_happening(e, &names.name(&e.instance), t)).collect();
    for run in runs {
        if let Some(end) = run.end.filter(|end| fresh(*end)) {
            let (kind, verb) = if run.outcome == Outcome::Failed { (HappeningKind::GateFailed, "failed") } else { (HappeningKind::GatePassed, "passed") };
            let cause = run.cause.as_ref().map(|c| format!(": {c}")).unwrap_or_default();
            if run.outcome != Outcome::Unknown {
                list.push(Happening {
                    t: end,
                    age: t.0.saturating_sub(end.0),
                    kind,
                    issue: None,
                    colour: None,
                    text: format!("gate {} {verb}{cause}", run.sub),
                });
            }
        }
    }
    for merge in merges.iter().filter(|m| fresh(m.t)) {
        let what = merge.pr.map_or_else(|| merge.commit.get(..8).unwrap_or(&merge.commit).to_owned(), |pr| format!("PR #{pr}"));
        list.push(Happening {
            t: merge.t,
            age: t.0.saturating_sub(merge.t.0),
            kind: HappeningKind::Landed,
            issue: None,
            colour: None,
            text: format!("{what} landed on main"),
        });
    }
    list.sort_by_key(|h| std::cmp::Reverse(h.t));
    list.truncate(HAPPENINGS);
    list
}

fn issue_happening(event: &IssueEvent, who: &str, t: Timestamp) -> Option<Happening> {
    let n = event.issue;
    let pr = event.kind.mr().map(|m| format!(" (PR #{m})")).unwrap_or_default();
    let note = event.note.as_ref().map(|n| format!(": {n}")).unwrap_or_default();
    let (kind, text) = match &event.kind {
        Kind::Claim { .. } => (HappeningKind::Claimed, format!("{who} claimed #{n}")),
        Kind::Work { .. } => (HappeningKind::Began, format!("{who} works on #{n}")),
        Kind::Submit { .. } => (HappeningKind::Submitted, format!("{who} submitted #{n}{pr}")),
        Kind::Sendback { .. } => (HappeningKind::SentBack, format!("#{n} sent back{note}")),
        Kind::Stop { reason: Some(swarmception_model::StopReason::Done), .. } => (HappeningKind::Done, format!("{who} stopped #{n}: done")),
        Kind::Stop { reason, .. } => {
            (HappeningKind::Stopped, format!("{who} stopped #{n}{}", reason.as_ref().map(|r| format!(" ({})", stop_reason(r))).unwrap_or_default()))
        }
        Kind::Release { .. } => (HappeningKind::Released, format!("#{n} released{note}")),
        Kind::Merge { .. } => (HappeningKind::Merged, format!("#{n} merged{pr}")),
        _ => return None,
    };
    Some(Happening { t: event.t, age: t.0.saturating_sub(event.t.0), kind, issue: Some(n), colour: Some(ticket_colour(n)), text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::tests::sample;
    use swarmception_model::{parse_file, parse_merge};

    fn at(text: &str) -> Timestamp {
        Timestamp::parse(text).unwrap()
    }

    fn sample_scene(t: &str) -> FactoryScene {
        let merges: Vec<MainMerge> =
            ["be043bc0\u{1f}1793420113\u{1f}Merge pull request #173 from mornycorp/170-spec"].into_iter().filter_map(parse_merge).collect();
        scene(&Activity::of(&sample()), &merges, at(t), DEFAULT_RECENT)
    }

    fn bench<'a>(scene: &'a FactoryScene, name: &str) -> &'a Bench {
        scene.benches.iter().find(|b| b.actor.name == name).unwrap()
    }

    /// Every crate of `scene`, wherever it is.
    fn all_crates(scene: &FactoryScene) -> Vec<&Crate> {
        let benches = scene.benches.iter().flat_map(|b| &b.crates);
        scene.upcoming.iter().chain(&scene.shelf).chain(benches).chain(&scene.dock).chain(&scene.gate.crates).chain(&scene.shipping.done).collect()
    }

    #[test]
    fn titled_crates_say_their_title_as_of_t() {
        let issues = crate::titles::tests::issues();
        let mut waiting = sample_scene("2026-10-31T04:10:00Z");
        let before = waiting.clone();
        titled(&mut waiting, &issues);
        let c = all_crates(&waiting).into_iter().find(|c| c.issue == 170).unwrap();
        assert_eq!(c.title.as_deref(), Some("The spec, written down"));
        assert_eq!(
            c.tooltip[..3],
            ["#170 The spec, written down", "open · type::task, area::docs · by cmaughan", "PR #173 Spec: the first draft"],
            "open at t: it closed later"
        );
        let was = all_crates(&before).into_iter().find(|c| c.issue == 170).unwrap();
        assert_eq!(c.tooltip[3..], was.tooltip[1..], "then the lines it had");
        let mut merged = sample_scene("2026-10-31T04:30:00Z");
        titled(&mut merged, &issues);
        let block = merged.shipping.blocks.iter().find(|b| b.pr == Some(173) && b.issue.is_some()).unwrap();
        assert_eq!(block.tooltip[0], "#170 The spec, written down");
        let mut none = sample_scene("2026-10-31T04:10:00Z");
        titled(&mut none, &swarmception_model::Issues::default());
        assert_eq!(none, before, "no issues, no change");
    }

    #[test]
    fn before_anything_every_ticket_is_up_next() {
        let scene = sample_scene("2026-10-31T02:00:00Z");
        assert_eq!(scene.upcoming.iter().map(|c| c.issue).collect::<Vec<_>>(), [168, 170], "soonest first");
        assert_eq!(scene.upcoming[0].age, 3_660, "#168's first event is at 03:01");
        assert_eq!(scene.counts.upcoming, 2);
        assert!(scene.benches.iter().all(|b| b.status == BenchStatus::Off && b.crates.is_empty()));
        assert_eq!(scene.benches.iter().map(|b| b.key.as_str()).collect::<Vec<_>>(), ["mini/worker-1", "mini/worker-2"], "a coordinator has no bench");
        assert_eq!(scene.gate.inspectors.len(), 1);
        assert_eq!(scene.gate.inspectors[0].shift, Shift::Off);
        assert!(scene.happenings.is_empty());
    }

    #[test]
    fn a_busy_moment_at_the_benches() {
        let scene = sample_scene("2026-10-31T04:06:10Z");
        let worker = bench(&scene, "worker-2");
        assert_eq!(worker.status, BenchStatus::Working);
        assert_eq!(worker.crates.iter().map(|c| (c.label.as_str(), c.state)).collect::<Vec<_>>(), [("#170", CrateState::Working)]);
        assert_eq!(worker.crates[0].since, at("2026-10-31T03:53:25Z"));
        assert_eq!(worker.actor.model.as_deref(), Some("claude-opus-5-5"));
        let helper = worker.helper.as_ref().unwrap();
        assert_eq!((helper.name.as_str(), helper.outcome, helper.age), ("ready", Outcome::Running, 10));
        // worker-1 works on #168 from 03:01, and nothing ends it until the coordinator ends its session.
        assert_eq!(bench(&scene, "worker-1").status, BenchStatus::Working);
        assert_eq!(scene.counts.at_benches, 2);
        assert!(worker.crates[0].tooltip.iter().all(|l| !l.starts_with("finished")), "nothing after t: {:?}", worker.crates[0].tooltip);
    }

    #[test]
    fn submitted_crates_wait_at_the_dock_then_go_through_the_gate() {
        let waiting = sample_scene("2026-10-31T04:10:00Z");
        assert_eq!(waiting.dock.iter().map(|c| (c.issue, c.state, c.worker.as_deref())).collect::<Vec<_>>(), [(170, CrateState::Waiting, Some("worker-2"))]);
        assert_eq!(bench(&waiting, "worker-2").status, BenchStatus::Paused, "its session paused at 04:10");
        let helper = bench(&waiting, "worker-2").helper.as_ref().unwrap();
        assert_eq!((helper.outcome, helper.age), (Outcome::Passed, 203), "its ready check passed 3 min 23 s before");

        let gating = sample_scene("2026-10-31T04:25:00Z");
        assert!(gating.dock.is_empty());
        assert_eq!(gating.gate.crates.iter().map(|c| (c.issue, c.state)).collect::<Vec<_>>(), [(170, CrateState::Gating)]);
        assert_eq!(gating.gate.running.len(), 1);
        let run = &gating.gate.running[0];
        assert_eq!((run.sub.as_str(), run.end, run.outcome), ("gate-pr173", None, Outcome::Running));
        assert_eq!(run.age, 300);
        assert_eq!(gating.gate.inspectors[0].shift, Shift::On);
        assert_eq!(gating.counts.gating, 1);
    }

    #[test]
    fn a_landing_ships_a_block_and_is_just_happened() {
        let scene = sample_scene("2026-10-31T04:30:00Z");
        assert_eq!(scene.counts.merged, 1);
        assert!(scene.gate.running.is_empty());
        assert_eq!((scene.gate.passed, scene.gate.failed), (1, 0));
        assert_eq!(scene.gate.recent[0].outcome, Outcome::Passed);
        assert_eq!(scene.shipping.blocks.iter().map(|b| (b.label.as_str(), b.issue)).collect::<Vec<_>>(), [("PR #173", Some(170))]);
        assert_eq!(scene.shipping.blocks[0].colour, ticket_colour(170));
        assert_eq!(scene.shipping.blocks[0].mark, "#170");
        assert_eq!((scene.shipping.landed, scene.shipping.total, scene.shipping.merged_tickets), (1, 1, 1));
        let kinds: Vec<HappeningKind> = scene.happenings.iter().map(|h| h.kind).collect();
        assert_eq!(kinds, [HappeningKind::GatePassed, HappeningKind::Merged], "newest first, the main merge at 04:15 too old");
        assert_eq!(scene.happenings[1].text, "#170 merged (PR #173)");
    }

    /// A gate that fails with a cause and sends a ticket back, which then goes through.
    const FAILING: &str = r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T01:00:00Z"}
{"event":"claim","issue":5,"mr":9,"t":"2026-10-31T01:00:00Z"}
{"event":"work","issue":5,"t":"2026-10-31T01:00:05Z"}
{"event":"submit","issue":5,"mr":9,"t":"2026-10-31T01:10:00Z"}
{"event":"work","issue":5,"t":"2026-10-31T01:30:00Z"}
{"event":"submit","issue":5,"mr":9,"t":"2026-10-31T01:40:00Z"}"#;
    const GATES: &str = r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T01:00:00Z"}
{"event":"sub.start","name":"gate","note":"local gate: #9 (#5); kind=smoke","sub":"gate-9","t":"2026-10-31T01:15:00Z"}
{"event":"sub.stop","note":"rc=1; cause=build; kind=smoke","sub":"gate-9","t":"2026-10-31T01:19:00Z"}
{"event":"sendback","issue":5,"mr":9,"note":"build: the API changed","sub":"gate-9","t":"2026-10-31T01:20:00Z"}
{"event":"merge","issue":5,"mr":9,"sub":"gate-pr9b","t":"2026-10-31T02:00:00Z"}"#;

    fn failing(t: &str) -> FactoryScene {
        let files = [parse_file("mini-w1", FAILING), parse_file("mini-c1", GATES)];
        scene(&Activity::of(&files), &[], at(t), DEFAULT_RECENT)
    }

    #[test]
    fn a_failing_gate_says_why_and_sends_the_crate_back() {
        let scene = failing("2026-10-31T01:21:00Z");
        let run = &scene.gate.recent[0];
        assert_eq!((run.sub.as_str(), run.outcome, run.cause.as_deref(), run.kind.as_deref()), ("gate-9", Outcome::Failed, Some("build"), Some("smoke")));
        assert_eq!(scene.gate.failed, 1);
        let back = &bench(&scene, "worker-1").crates[0];
        assert_eq!((back.state, back.note.as_deref()), (CrateState::SentBack, Some("build: the API changed")));
        assert_eq!(bench(&scene, "worker-1").status, BenchStatus::SentBack);
        let texts: Vec<&str> = scene.happenings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, ["#5 sent back: build: the API changed", "gate gate-9 failed: build"]);
    }

    #[test]
    fn a_gate_with_no_start_is_read_from_its_merge() {
        let scene = failing("2026-10-31T02:05:00Z");
        let run = &scene.gate.recent[0];
        assert_eq!((run.sub.as_str(), run.outcome, run.inferred, run.end), ("gate-pr9b", Outcome::Passed, true, Some(at("2026-10-31T02:00:00Z"))));
        assert_eq!(run.refs, [9]);
        assert_eq!((scene.gate.passed, scene.gate.failed), (1, 1));
        assert_eq!(scene.shipping.blocks.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["PR #9"], "no main: the tickets' merges are the blocks");
        assert_eq!(scene.shipping.total, 1);
    }

    #[test]
    fn a_helper_that_never_stopped_runs_only_while_its_worker_does() {
        let worker = r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T01:00:00Z"}
{"event":"sub.start","name":"ready","sub":"ready-1","t":"2026-10-31T01:10:00Z"}
{"event":"session.stop","t":"2026-10-31T01:20:00Z"}
{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}"#;
        let activity = Activity::of(&[parse_file("mini-w1", worker)]);
        let at = |t: &str| scene(&activity, &[], self::at(t), DEFAULT_RECENT);
        let during = at("2026-10-31T01:15:00Z");
        assert_eq!(during.benches[0].helper.as_ref().map(|h| h.outcome), Some(Outcome::Running));
        let stale = at("2026-10-31T04:20:00Z");
        assert_eq!(
            stale.benches[0].helper.as_ref().map(|h| (h.outcome, h.age)),
            Some((Outcome::Unknown, 11_400)),
            "its next session: three hours on, no stop: unknown"
        );
        assert_eq!(at("2026-10-31T02:00:00Z").benches[0].helper, None, "its session ended: no helper runs, and none ended");
    }

    #[test]
    fn released_tickets_go_on_the_shelf_and_done_ones_ship_without_a_merge() {
        let worker = r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T01:00:00Z"}
{"event":"claim","issue":6,"t":"2026-10-31T01:41:00Z"}
{"event":"release","issue":6,"t":"2026-10-31T01:50:00Z"}
{"event":"claim","issue":58,"t":"2026-10-31T02:00:00Z"}
{"event":"stop","issue":58,"reason":"done","note":"review filed as #66-#73","t":"2026-10-31T02:10:00Z"}"#;
        let activity = Activity::of(&[parse_file("mini-w1", worker)]);
        let scene = scene(&activity, &[], at("2026-10-31T02:20:00Z"), DEFAULT_RECENT);
        assert_eq!(scene.shelf.iter().map(|c| (c.issue, c.since)).collect::<Vec<_>>(), [(6, at("2026-10-31T01:50:00Z"))]);
        assert_eq!(scene.shipping.done.iter().map(|c| (c.issue, c.note.as_deref())).collect::<Vec<_>>(), [(58, Some("review filed as #66-#73"))]);
        assert_eq!((scene.counts.shelved, scene.counts.done, scene.shipping.done_count), (1, 1, 1));
        assert_eq!(bench(&scene, "worker-1").status, BenchStatus::Idle);
    }

    #[test]
    fn the_same_events_the_same_scene_and_nothing_reads_as_nothing() {
        assert_eq!(sample_scene("2026-10-31T04:25:00Z"), sample_scene("2026-10-31T04:25:00Z"));
        let empty = scene(&Activity::of(&[]), &[], Timestamp(0), -5);
        assert!(empty.benches.is_empty() && empty.upcoming.is_empty() && empty.shipping.blocks.is_empty());
        assert_eq!((empty.recent, empty.first), (1, None));
    }

    #[test]
    fn a_journey_through_the_stations_with_the_time_at_each() {
        let files = [parse_file("mini-w1", FAILING), parse_file("mini-c1", GATES)];
        let activity = Activity::of(&files);
        // Bench 01:00-01:10, dock 01:10-01:15, gate 01:15-01:19, dock 01:19-01:20 (sent back),
        // bench 01:20-01:40, dock 01:40-02:00, merged at 02:00 by a gate that logged no start.
        let done = journey(&activity, at("2026-10-31T02:05:00Z"), 5).unwrap();
        assert_eq!(done.text, "bench 10m → dock 5m → gate 4m → dock 1m → bench 20m → dock 20m → main");
        assert_eq!(done.legs[2].start, at("2026-10-31T01:15:00Z"));
        let midway = journey(&activity, at("2026-10-31T01:17:00Z"), 5).unwrap();
        assert_eq!(midway.text, "bench 10m → dock 5m → gate 2m", "nothing after t");
        assert_eq!(journey(&activity, at("2026-10-31T02:05:00Z"), 99), None);
    }

    #[test]
    fn a_merge_no_ticket_names_is_a_grey_brick_marked_with_its_pr() {
        let merges: Vec<MainMerge> = [
            "be043bc0\u{1f}1793420113\u{1f}Merge pull request #173 from mornycorp/170-spec",
            "15bad2c0\u{1f}1793420255\u{1f}Merge pull request #174 from mornycorp/progress-7",
            "a1b2c3d4e5\u{1f}1793420300\u{1f}Merge branch 'setup'",
        ]
        .into_iter()
        .filter_map(parse_merge)
        .collect();
        let scene = scene(&Activity::of(&sample()), &merges, at("2026-10-31T05:00:00Z"), DEFAULT_RECENT);
        let marks: Vec<(&str, Rgb)> = scene.shipping.blocks.iter().map(|b| (b.mark.as_str(), b.colour)).collect();
        assert_eq!(marks, [("#170", ticket_colour(170)), ("!174", NO_TICKET), ("a1b2c3d", NO_TICKET)]);
        let setup = &scene.shipping.blocks[1];
        assert_eq!(setup.issue, None);
        assert_eq!(setup.tooltip, ["Merge pull request #174 from mornycorp/progress-7", "15bad2c0 · 2026-10-31T04:17:35Z", "no ticket's merge names it"]);
    }

    #[test]
    fn colours_short_times_and_note_fields() {
        assert_eq!(ticket_colour(170), ticket_colour(170));
        assert_ne!(ticket_colour(170), ticket_colour(171));
        assert_ne!(ticket_colour(u64::MAX), [0, 0, 0]);
        assert_eq!([short(45), short(720), short(3_900), short(90_000), short(-1)], ["45s", "12m", "1h05", "1d01h", "0s"]);
        assert_eq!(note_field("rc=1; cause=build; kind=smoke (unsure)", "cause").as_deref(), Some("build"));
        assert_eq!(note_field("rc=1; kind=smoke (unsure)", "kind").as_deref(), Some("smoke (unsure)"));
        assert_eq!(note_field("rc=0", "cause"), None);
        assert_eq!(note_field("cause=", "cause"), None);
    }
}
