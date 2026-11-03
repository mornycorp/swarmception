//! The swarm's statistics over a window of time (the owner's direction, #20): the key one is
//! **tickets landed per active hour**, the throughput while the swarm works, with its quiet and
//! off periods left out. Every figure is defined here, once:
//!
//! - **On shift**: the time at least one worker (any agent but a coordinator) has a session
//!   going, from its `session.start` to its `session.stop` (or the last event, when nothing
//!   stopped it), its pauses left out.
//! - **A quiet gap**: the time between two consecutive events, of any agent, more than
//!   [`QUIET`] (15 minutes) apart. The whole gap is quiet, not what is over 15 minutes.
//! - **Active time**: on-shift time less every quiet gap, within the window. A night off with a
//!   session left open, or an hour mid-shift with no event, is not active.
//! - **Landed**: the tickets whose merge (a `merge` event) falls in the window.
//!   **Landed per active hour** is landed ÷ the active hours; none with no active time.
//! - **The hourly series**: per clock hour (UTC) of the window, the tickets landed in it, its
//!   active time and their ratio: a sparkline's points.
//! - **Claim → merge** and **the wait at the dock** (the last submit → merge): medians over the
//!   tickets landed in the window, wall-clock seconds.
//! - **Gate runs**: the coordinator's gates and the workers' own `ready` checks that started in
//!   the window, as [`crate::gates`] defines a run and its result, once, for `gates.list` too
//!   (#75): each passed (`rc=0`, or no `rc` but a merge naming it while it ran), failed (another
//!   `rc`, by its note's `cause=…`; `unstated` without one), has no result (it ended without
//!   saying, or never finished, #42) or is still running. A gate read from its merges and send
//!   backs, with no `sub.start` logged, counts, and `inferred` says how many were. The pass rate
//!   is passed ÷ (passed + failed).
//! - **Sendbacks**: the send-backs in the window (a `sendback`, or a gate's failure for the change's
//!   cause: [`crate::sendbacks`], #106, each once), and their count ÷ the tickets landed.
//! - **Work in progress**: the mean number of tickets *working* (from `work` to its submit,
//!   send-back, stop, release or merge) over the active time, and of tickets *in flight*
//!   (claimed, working, waiting for merge or sent back): each ticket's time in the phase within
//!   the active time, summed, ÷ the active time.
//! - **A worker's utilisation** (by machine and name, across its instances): the time its
//!   tickets were *working* ÷ its time on shift, both less the quiet gaps, within the window.
//!
//! The window defaults to the whole run: the first event to the last. Its ends are both
//! included for instants (a merge at its end lands in it). Pure: no I/O, no clock.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{Kind, MainMerge, Role, Timestamp};

use crate::activity::{Activity, Agent, Sub};
use crate::gates::{Bounds, Figures, Lane, lane_figures};
use crate::tickets::{Phase, Ticket};

/// A gap between events longer than this, in seconds, is quiet: 15 minutes (owner, 2026-11-01,
/// #20).
pub const QUIET: i64 = 15 * 60;
/// The most hours the hourly series holds: the window's last 92 days.
pub const MAX_HOURS: usize = 24 * 92;
const HOUR: i64 = 3_600;

/// The window asked for; an end left out is the run's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Window {
    pub start: Option<Timestamp>,
    pub end: Option<Timestamp>,
}

/// How a lane of gate runs went: `gates.list`'s counts of it over the same window (#75).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Gates {
    /// The runs that started in the window.
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    /// Ended with no `rc` and no merge naming them (a gate that never logged its stop among
    /// them, #42).
    pub no_result: usize,
    /// Nothing ended them, and their session goes on.
    pub running: usize,
    /// Of the runs, those read from merges and send backs: no `sub.start` logged.
    pub inferred: usize,
    /// Passed ÷ (passed + failed); none when neither.
    pub pass_rate: Option<f64>,
    /// The failures by their note's `cause=…` (`unstated` without one) and, for a `ready`
    /// check, its `step=…` ([`failure_label`]: `build/clippy`), most first.
    pub causes: Vec<Cause>,
}

impl From<&Figures> for Gates {
    fn from(figures: &Figures) -> Self {
        Self {
            runs: figures.runs,
            passed: figures.passed,
            failed: figures.failed,
            no_result: figures.no_result,
            running: figures.running,
            inferred: figures.inferred,
            pass_rate: figures.pass_rate,
            causes: figures.causes.clone(),
        }
    }
}

/// Failures of one cause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cause {
    pub cause: String,
    pub count: usize,
}

/// One clock hour of the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hour {
    /// The hour's start, UTC.
    pub start: Timestamp,
    pub landed: usize,
    pub active_seconds: i64,
    /// Landed ÷ its active hours; none with no active time.
    pub landed_per_active_hour: Option<f64>,
}

/// One worker, by machine and name, across its instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStats {
    pub machine: String,
    pub name: String,
    pub instances: usize,
    /// Its sessions less their pauses and the quiet gaps, in the window.
    pub on_shift_seconds: i64,
    /// The time its tickets were working, within its time on shift.
    pub working_seconds: i64,
    /// Working ÷ on shift; none with no time on shift.
    pub utilisation: Option<f64>,
    /// The tickets landed in the window that it held last.
    pub landed: usize,
}

/// The swarm's statistics over a window (the module's docs define each).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// The window, its ends included.
    pub start: Timestamp,
    pub end: Timestamp,
    /// A gap between events longer than this, in seconds, is quiet.
    pub quiet_after_seconds: i64,
    /// The window's length.
    pub span_seconds: i64,
    /// The time at least one worker had a session going, its pauses left out.
    pub on_shift_seconds: i64,
    /// On shift, less the quiet gaps.
    pub active_seconds: i64,
    /// The quiet gaps that cut into the time on shift.
    pub quiet_gaps: usize,
    /// The tickets merged in the window.
    pub landed: usize,
    /// The headline: landed ÷ the active hours.
    pub landed_per_active_hour: Option<f64>,
    /// Every merge commit onto main in the window, tickets' or not.
    pub main_merges: usize,
    /// Each clock hour of the window, from the first with active time or a landing to the last.
    pub hours: Vec<Hour>,
    /// Whether the series was cut to its last [`MAX_HOURS`].
    pub hours_cut: bool,
    /// The tickets first claimed in the window.
    pub claimed: usize,
    /// Medians over the tickets landed, in seconds: claim → merge, and the last submit → merge.
    pub median_claim_to_merge_seconds: Option<i64>,
    pub median_dock_wait_seconds: Option<i64>,
    pub sendbacks: usize,
    /// Sendbacks ÷ the tickets landed.
    pub sendbacks_per_landed: Option<f64>,
    /// The mean tickets working, and in flight, over the active time.
    pub mean_working: Option<f64>,
    pub mean_in_flight: Option<f64>,
    /// The coordinator's gates.
    pub gates: Gates,
    /// The workers' own gates before they submit (`ready`).
    pub ready: Gates,
    pub workers: Vec<WorkerStats>,
}

/// The statistics of `activity`'s `tickets` and `main`'s merges over `window`.
#[must_use]
pub fn stats(activity: &Activity, tickets: &[Ticket], merges: &[MainMerge], window: Window) -> Stats {
    let (first, last) = activity.extent.unwrap_or_default();
    let start = window.start.unwrap_or(first);
    let end = window.end.unwrap_or(last).max(start);
    let span = Intervals::span(start.0, end.0);
    let quiet = quiet_gaps(&activity.times);
    let workers: Vec<&Agent> = activity.agents.iter().filter(|a| a.role != Role::Coordinator).collect();
    let on_shift = Intervals::of(workers.iter().flat_map(|a| shift(a))).intersect(&span);
    let active = on_shift.subtract(&quiet);
    let within = |t: Timestamp| start <= t && t <= end;
    let landed: Vec<&Ticket> = tickets.iter().filter(|t| t.merged.as_ref().is_some_and(|m| within(m.t))).collect();
    let (hours, hours_cut) = hourly(start, end, &active, &landed);
    let sendbacks = activity.issue_events.iter().filter(|e| matches!(e.kind, Kind::Sendback { .. }) && within(e.t)).count();
    let runs = crate::gates::runs(activity, tickets, merges);
    let bounds = Bounds::of(activity, window);
    let lane = |lane: Lane| Gates::from(&lane_figures(&runs, lane, bounds));
    Stats {
        start,
        end,
        quiet_after_seconds: QUIET,
        span_seconds: end.0.saturating_sub(start.0),
        on_shift_seconds: on_shift.measure(),
        active_seconds: active.measure(),
        quiet_gaps: quiet.0.iter().filter(|&&(a, b)| on_shift.measure_in(a, b) > 0).count(),
        landed: landed.len(),
        landed_per_active_hour: per_hour(landed.len(), active.measure()),
        main_merges: merges.iter().filter(|m| within(m.t)).count(),
        hours,
        hours_cut,
        claimed: tickets.iter().filter(|t| t.claimed.is_some_and(within)).count(),
        median_claim_to_merge_seconds: median(
            landed.iter().filter_map(|t| Some(t.merged.as_ref()?.t.0.saturating_sub(t.claimed?.0))).filter(|d| *d >= 0).collect(),
        ),
        median_dock_wait_seconds: median(
            landed.iter().filter_map(|t| Some(t.merged.as_ref()?.t.0.saturating_sub(t.finished?.0))).filter(|d| *d >= 0).collect(),
        ),
        sendbacks,
        sendbacks_per_landed: ratio(sendbacks, landed.len()),
        mean_working: mean_in(tickets, &active, &[Phase::Working]),
        mean_in_flight: mean_in(tickets, &active, &[Phase::Claimed, Phase::Working, Phase::Waiting, Phase::SentBack]),
        gates: lane(Lane::Gate),
        ready: lane(Lane::Ready),
        workers: worker_stats(&workers, tickets, &active, &landed),
    }
}

/// A worker's own gate before it submits.
pub(crate) fn is_ready(sub: &Sub) -> bool {
    !sub.is_gate() && (sub.sub.starts_with("ready") || sub.name.as_deref() == Some("ready"))
}

/// An agent's sessions, less their pauses.
pub(crate) fn shift(agent: &Agent) -> Vec<(i64, i64)> {
    agent
        .sessions
        .iter()
        .flat_map(|s| {
            let pauses = Intervals::of(s.pauses.iter().map(|p| (p.start.0, p.end.0)));
            Intervals::span(s.span.start.0, s.span.end.0).subtract(&pauses).0
        })
        .collect()
}

/// Every gap between consecutive events longer than [`QUIET`].
pub(crate) fn quiet_gaps(times: &[Timestamp]) -> Intervals {
    Intervals::of(times.windows(2).filter_map(|w| match w {
        [a, b] if b.0.saturating_sub(a.0) > QUIET => Some((a.0, b.0)),
        _ => None,
    }))
}

/// Each clock hour of the window from the first with active time or a landing to the last, cut
/// to the last [`MAX_HOURS`].
fn hourly(start: Timestamp, end: Timestamp, active: &Intervals, landed: &[&Ticket]) -> (Vec<Hour>, bool) {
    let floor = |t: i64| t.div_euclid(HOUR).saturating_mul(HOUR);
    let merged: Vec<i64> = landed.iter().filter_map(|t| Some(t.merged.as_ref()?.t.0)).collect();
    let busy = active.0.iter().flat_map(|&(a, b)| [a, b.saturating_sub(1)]).chain(merged.iter().copied());
    let (Some(lo), Some(hi)) = (busy.clone().min(), busy.max()) else { return (Vec::new(), false) };
    let (lo, hi) = (floor(lo.max(start.0)), floor(hi.min(end.0)));
    let count = hi.saturating_sub(lo) / HOUR;
    let cut = usize::try_from(count).map_or(true, |n| n >= MAX_HOURS);
    let lo = if cut { hi.saturating_sub(HOUR * (MAX_HOURS as i64 - 1)) } else { lo };
    let mut hours = Vec::new();
    let mut at = lo;
    while at <= hi && hours.len() < MAX_HOURS {
        let next = at.saturating_add(HOUR);
        let active_seconds = active.measure_in(at, next);
        let landed = merged.iter().filter(|&&t| at <= t && t < next).count();
        hours.push(Hour { start: Timestamp(at), landed, active_seconds, landed_per_active_hour: per_hour(landed, active_seconds) });
        if next == at {
            break;
        }
        at = next;
    }
    (hours, cut)
}

/// The mean number of tickets in one of `phases` over `active`.
fn mean_in(tickets: &[Ticket], active: &Intervals, phases: &[Phase]) -> Option<f64> {
    let total: i64 = tickets
        .iter()
        .map(|t| Intervals::of(t.stretches.iter().filter(|s| phases.contains(&s.phase)).map(|s| (s.start.0, s.end.0))).intersect(active).measure())
        .fold(0, i64::saturating_add);
    fraction(total, active.measure())
}

/// The cause a stop note names: `cause=<word>`, up to a `;` or a space.
#[must_use]
pub fn cause_of(note: &str) -> Option<String> {
    let at = note.find("cause=")?;
    let rest = note.get(at + "cause=".len()..)?;
    let cause: String = rest.chars().take_while(|c| *c != ';' && !c.is_whitespace()).take(64).collect();
    (!cause.is_empty()).then_some(cause)
}

/// The `ci` step a failed `ready` check's stop note names: `step=<word>` (swarm-core's `ready`
/// writes `rc=<n>; cause=<cause>; step=<step>`, #77), a field of its own; none in an older note.
#[must_use]
pub fn step_of(note: &str) -> Option<String> {
    let step = note.split(|c: char| c == ';' || c.is_whitespace()).find_map(|field| field.strip_prefix("step="))?;
    let step: String = step.chars().take(64).collect();
    (!step.is_empty()).then_some(step)
}

/// What a failure reads as in the causes: its cause (`unstated` without one), and the `ci` step it
/// failed at when that says more: `build/clippy`, `other/fmt`, `test` (not `test/test`).
#[must_use]
pub fn failure_label(cause: Option<&str>, step: Option<&str>) -> String {
    let cause = cause.unwrap_or("unstated");
    match step {
        Some(step) if step != cause => format!("{cause}/{step}"),
        _ => cause.to_owned(),
    }
}

fn worker_stats(workers: &[&Agent], tickets: &[Ticket], active: &Intervals, landed: &[&Ticket]) -> Vec<WorkerStats> {
    let mut by_worker: BTreeMap<(&str, &str), Vec<&Agent>> = BTreeMap::new();
    for agent in workers {
        by_worker.entry((agent.machine.as_str(), agent.name.as_str())).or_default().push(agent);
    }
    by_worker
        .into_iter()
        .map(|((machine, name), agents)| {
            let ours = |instance: &str| agents.iter().any(|a| a.instance == instance);
            let on_shift = Intervals::of(agents.iter().flat_map(|a| shift(a))).intersect(active);
            let working = tickets.iter().flat_map(|t| &t.stretches).filter(|s| s.phase == Phase::Working && ours(&s.by)).map(|s| (s.start.0, s.end.0));
            let working = Intervals::of(working).intersect(&on_shift).measure();
            WorkerStats {
                machine: machine.to_owned(),
                name: name.to_owned(),
                instances: agents.len(),
                on_shift_seconds: on_shift.measure(),
                working_seconds: working,
                utilisation: fraction(working, on_shift.measure()),
                landed: landed.iter().filter(|t| t.worker.as_deref().is_some_and(ours)).count(),
            }
        })
        .collect()
}

/// `count` per hour of `seconds`; none with no time.
fn per_hour(count: usize, seconds: i64) -> Option<f64> {
    (seconds > 0).then(|| count as f64 * HOUR as f64 / seconds as f64)
}

pub(crate) fn ratio(a: usize, b: usize) -> Option<f64> {
    (b > 0).then(|| a as f64 / b as f64)
}

pub(crate) fn fraction(a: i64, b: i64) -> Option<f64> {
    (b > 0).then(|| a as f64 / b as f64)
}

/// The median, the mean of the middle two of an even count; none of nothing.
pub(crate) fn median(mut values: Vec<i64>) -> Option<i64> {
    values.sort_unstable();
    let mid = values.len() / 2;
    match (values.len() % 2, values.get(mid), mid.checked_sub(1).and_then(|m| values.get(m))) {
        (1, Some(&m), _) => Some(m),
        (0, Some(&b), Some(&a)) => Some(a + (b - a) / 2),
        _ => None,
    }
}

/// A set of seconds: sorted, disjoint, non-empty half-open spans `[a, b)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Intervals(pub(crate) Vec<(i64, i64)>);

impl Intervals {
    /// The union of `spans`; empty and backward ones are nothing.
    pub(crate) fn of(spans: impl IntoIterator<Item = (i64, i64)>) -> Self {
        let mut spans: Vec<(i64, i64)> = spans.into_iter().filter(|(a, b)| a < b).collect();
        spans.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::with_capacity(spans.len());
        for (a, b) in spans {
            match merged.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        Self(merged)
    }

    pub(crate) fn span(a: i64, b: i64) -> Self {
        Self::of([(a, b)])
    }

    pub(crate) fn measure(&self) -> i64 {
        self.0.iter().map(|(a, b)| b.saturating_sub(*a)).fold(0, i64::saturating_add)
    }

    /// The measure of the part within `[a, b)`.
    pub(crate) fn measure_in(&self, a: i64, b: i64) -> i64 {
        let from = self.0.partition_point(|&(_, end)| end <= a);
        self.0.get(from..).unwrap_or_default().iter().take_while(|&&(s, _)| s < b).map(|&(s, e)| e.min(b).saturating_sub(s.max(a))).fold(0, i64::saturating_add)
    }

    pub(crate) fn intersect(&self, other: &Self) -> Self {
        let (mut i, mut j, mut out) = (0, 0, Vec::new());
        while let (Some(&(a, b)), Some(&(c, d))) = (self.0.get(i), other.0.get(j)) {
            let (lo, hi) = (a.max(c), b.min(d));
            if lo < hi {
                out.push((lo, hi));
            }
            if b < d {
                i += 1;
            } else {
                j += 1;
            }
        }
        Self(out)
    }

    pub(crate) fn subtract(&self, other: &Self) -> Self {
        let mut out = Vec::new();
        let mut j = 0;
        for &(a, b) in &self.0 {
            while other.0.get(j).is_some_and(|&(_, d)| d <= a) {
                j += 1;
            }
            let mut from = a;
            for &(c, d) in other.0.get(j..).unwrap_or_default().iter().take_while(|&&(c, _)| c < b) {
                if c > from {
                    out.push((from, c));
                }
                from = from.max(d);
            }
            if from < b {
                out.push((from, b));
            }
        }
        Self(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tickets::tickets;
    use swarmception_model::{AgentFile, parse_file};

    const BASE: i64 = 1_793_404_800; // 2026-10-31T00:00:00Z

    /// The time `h` hours and `m` minutes after 2026-10-31T00:00:00Z (past 23, the next day).
    fn at(h: i64, m: i64) -> Timestamp {
        Timestamp(BASE + h * HOUR + m * 60)
    }

    fn line(h: i64, m: i64, body: &str) -> String {
        format!("{{\"t\":\"{}\",{body}}}", at(h, m))
    }

    /// An event every `step` minutes from `h0:00` to `h1:00`, both included.
    fn beats(h0: i64, h1: i64, step: i64) -> Vec<String> {
        (0..=(h1 - h0) * 60 / step).map(|i| line(h0, i * step, r#""event":"beat""#)).collect()
    }

    fn start(h: i64, m: i64, name: &str, role: &str) -> String {
        line(h, m, &format!(r#""event":"session.start","machine":"mini","name":"{name}","role":"{role}""#))
    }

    /// A file of `lines`, put in time order (a stable sort: a tie keeps its order).
    fn file(instance: &str, mut lines: Vec<String>) -> AgentFile {
        lines.sort_by_key(|l| l.get(6..26).map(str::to_owned));
        parse_file(instance, &lines.join("\n"))
    }

    fn run(files: &[AgentFile], window: Window) -> Stats {
        let activity = Activity::of(files);
        stats(&activity, &tickets(&activity), &[], window)
    }

    fn whole(files: &[AgentFile]) -> Stats {
        run(files, Window::default())
    }

    #[test]
    fn a_night_off_is_not_active() {
        let mut worker = vec![start(9, 0, "worker-1", "worker")];
        worker.extend(beats(9, 10, 10));
        worker.extend(beats(33, 34, 10));
        for (issue, h, m) in [(1, 9, 0), (2, 9, 40), (3, 33, 0), (4, 33, 30)] {
            worker.push(line(h, m, &format!(r#""event":"claim","issue":{issue}"#)));
            worker.push(line(h, m + 1, &format!(r#""event":"work","issue":{issue}"#)));
            worker.push(line(h, m + 10, &format!(r#""event":"submit","issue":{issue},"mr":{}"#, issue + 100)));
        }
        let merges = [(1, 9, 35), (2, 9, 55), (3, 33, 20), (4, 33, 50)];
        let coordinator: Vec<String> = merges.iter().map(|(i, h, m)| line(*h, *m, &format!(r#""event":"merge","issue":{i}"#))).collect();
        let stats = whole(&[file("mini-aa", worker), file("mini-cc", coordinator)]);
        assert_eq!(stats.on_shift_seconds, 25 * HOUR, "the session left open overnight");
        assert_eq!(stats.active_seconds, 2 * HOUR, "the night is one quiet gap");
        assert_eq!(stats.quiet_gaps, 1);
        assert_eq!(stats.landed, 4);
        assert_eq!(stats.landed_per_active_hour, Some(2.0));
        assert_eq!(stats.claimed, 4);
        assert_eq!(stats.median_dock_wait_seconds, Some(10 * 60), "waits of 25, 5, 10 and 10 minutes");
        assert_eq!(stats.median_claim_to_merge_seconds, Some(20 * 60), "35, 15, 20 and 20 minutes");
    }

    #[test]
    fn a_quiet_hour_mid_shift_is_cut_and_pauses_are_off_shift() {
        let mut worker = vec![start(9, 0, "worker-1", "worker")];
        worker.extend(beats(9, 10, 10));
        for m in [0, 15, 30, 45] {
            worker.push(line(11, m, r#""event":"beat""#));
        }
        worker.push(line(11, 30, r#""event":"session.pause""#));
        worker.push(line(11, 45, r#""event":"session.resume""#));
        worker.push(line(12, 0, r#""event":"session.stop""#));
        let stats = whole(&[file("mini-aa", worker)]);
        assert_eq!(stats.on_shift_seconds, 3 * HOUR - 15 * 60, "less the pause");
        assert_eq!(stats.active_seconds, 2 * HOUR - 15 * 60, "10:00 to 11:00 is quiet; a 15 minute gap isn't");
        assert_eq!(stats.quiet_gaps, 1);
        assert_eq!((stats.landed, stats.landed_per_active_hour), (0, Some(0.0)));
    }

    /// Worker-1 on shift 09:00 to 10:00 working #1 till 09:30; worker-2 on shift 09:30 to 10:30
    /// working #2 till 10:15; the coordinator from 08:00, merging both.
    fn overlapping() -> Vec<AgentFile> {
        let mut one = vec![start(9, 0, "worker-1", "worker")];
        one.extend(beats(9, 10, 5));
        one.extend([
            line(9, 0, r#""event":"claim","issue":1"#),
            line(9, 0, r#""event":"work","issue":1"#),
            line(9, 30, r#""event":"submit","issue":1,"mr":11"#),
        ]);
        one.push(line(10, 0, r#""event":"session.stop""#));
        let mut two = vec![start(9, 30, "worker-2", "worker")];
        two.extend(beats(9, 10, 5).into_iter().skip(6));
        two.extend([line(10, 5, r#""event":"beat""#), line(10, 10, r#""event":"beat""#)]);
        two.extend([
            line(9, 30, r#""event":"claim","issue":2"#),
            line(9, 30, r#""event":"work","issue":2"#),
            line(10, 15, r#""event":"submit","issue":2,"mr":12"#),
        ]);
        two.push(line(10, 30, r#""event":"session.stop""#));
        let coordinator = vec![
            start(8, 0, "coordinator", "coordinator"),
            line(10, 20, r#""event":"merge","issue":1,"mr":11"#),
            line(10, 25, r#""event":"merge","issue":2,"mr":12"#),
        ];
        vec![file("mini-aa", one), file("mini-bb", two), file("mini-cc", coordinator)]
    }

    #[test]
    fn two_overlapping_workers_count_once() {
        let stats = whole(&overlapping());
        assert_eq!(stats.start, at(8, 0));
        assert_eq!(stats.on_shift_seconds, 90 * 60, "the coordinator's session isn't a worker's");
        assert_eq!(stats.active_seconds, 90 * 60);
        assert_eq!(stats.landed, 2);
        assert_eq!(stats.landed_per_active_hour, Some(2.0 / 1.5));
        assert_eq!(stats.median_claim_to_merge_seconds, Some((80 * 60 + 55 * 60) / 2));
        assert_eq!(stats.median_dock_wait_seconds, Some((50 * 60 + 10 * 60) / 2));
        let mean = stats.mean_working.unwrap();
        assert!((mean - 75.0 / 90.0).abs() < 1e-9, "{mean}");
        let in_flight = stats.mean_in_flight.unwrap();
        assert!((in_flight - (80.0 + 55.0) / 90.0).abs() < 1e-9, "#1 till its merge at 10:20, #2 at 10:25, within 09:00 to 10:30: {in_flight}");
        let workers: Vec<(&str, i64, i64, Option<f64>, usize)> =
            stats.workers.iter().map(|w| (w.name.as_str(), w.on_shift_seconds, w.working_seconds, w.utilisation, w.landed)).collect();
        assert_eq!(workers, [("worker-1", 3600, 1800, Some(0.5), 1), ("worker-2", 3600, 2700, Some(0.75), 1)]);
    }

    #[test]
    fn the_hourly_series_and_a_window() {
        let stats = whole(&overlapping());
        let hours: Vec<(Timestamp, usize, i64, Option<f64>)> =
            stats.hours.iter().map(|h| (h.start, h.landed, h.active_seconds, h.landed_per_active_hour)).collect();
        assert_eq!(hours, [(at(9, 0), 0, 3600, Some(0.0)), (at(10, 0), 2, 1800, Some(4.0))], "from the first busy hour, not the coordinator's start");
        assert!(!stats.hours_cut);
        let early = run(&overlapping(), Window { start: Some(at(9, 0)), end: Some(at(9, 45)) });
        assert_eq!((early.span_seconds, early.active_seconds, early.landed, early.claimed), (45 * 60, 45 * 60, 0, 2));
        assert_eq!(early.landed_per_active_hour, Some(0.0));
        assert_eq!(early.workers[1].on_shift_seconds, 15 * 60);
        let at_end = run(&overlapping(), Window { start: Some(at(10, 0)), end: Some(at(10, 25)) });
        assert_eq!(at_end.landed, 2, "a merge at the window's end lands in it");
    }

    #[test]
    fn gates_pass_rate_and_causes() {
        let gate = |sub: &str, h: i64, stop: Option<&str>| {
            let mut lines = vec![line(h, 0, &format!(r#""event":"sub.start","name":"gate","sub":"{sub}""#))];
            lines.extend(stop.map(|note| line(h, 5, &format!(r#""event":"sub.stop","note":"{note}","sub":"{sub}""#))));
            lines
        };
        let mut coordinator = vec![start(0, 0, "coordinator", "coordinator")];
        coordinator.extend(gate("gate-a", 1, Some("rc=0; kind=smoke")));
        coordinator.extend(gate("gate-b", 2, Some("rc=1; cause=build; kind=smoke")));
        coordinator.extend(gate("gate-c", 3, Some("rc=1")));
        coordinator.extend(gate("gate-d", 4, Some("merged #15")));
        coordinator.push(line(4, 3, r#""event":"merge","issue":5,"mr":15,"sub":"gate-d""#));
        coordinator.push(line(4, 1, r#""event":"sendback","issue":5,"mr":15"#));
        coordinator.extend(gate("gate-e", 5, None));
        let worker = vec![
            start(0, 0, "worker-1", "worker"),
            line(1, 0, r#""event":"sub.start","name":"ready","sub":"ready-1""#),
            line(1, 1, r#""event":"sub.stop","note":"rc=0","sub":"ready-1""#),
            line(2, 0, r#""event":"sub.start","name":"ready","sub":"ready-2""#),
            line(2, 1, r#""event":"sub.stop","note":"rc=1; cause=test","sub":"ready-2""#),
        ];
        let files = [file("mini-cc", coordinator), file("mini-aa", worker)];
        let stats = whole(&files);
        let gates = &stats.gates;
        assert_eq!((gates.runs, gates.passed, gates.failed, gates.no_result, gates.pass_rate), (5, 2, 2, 0, Some(0.5)), "gate-d passed by its merge");
        assert_eq!((gates.running, gates.inferred), (1, 0), "gate-e: its session goes on");
        let causes: Vec<(&str, usize)> = gates.causes.iter().map(|c| (c.cause.as_str(), c.count)).collect();
        assert_eq!(causes, [("build", 1), ("unstated", 1)]);
        let ready = &stats.ready;
        assert_eq!((ready.runs, ready.passed, ready.failed, ready.pass_rate), (2, 1, 1, Some(0.5)));
        assert_eq!(ready.causes, [Cause { cause: "test".into(), count: 1 }]);
        assert_eq!((stats.sendbacks, stats.sendbacks_per_landed), (1, Some(1.0)));
        let late = run(&files, Window { start: Some(at(3, 0)), end: None });
        assert_eq!((late.gates.runs, late.ready.runs), (3, 0), "by when they started");
    }

    #[test]
    fn causes_read_from_notes() {
        assert_eq!(cause_of("rc=1; cause=build; kind=smoke").as_deref(), Some("build"));
        assert_eq!(cause_of("rc=1; cause=test").as_deref(), Some("test"));
        assert_eq!(cause_of("rc=1; cause="), None);
        assert_eq!(cause_of("rc=1"), None);
    }

    /// #77: a `ready` stop's `step=`, a field of its own; an older note names none.
    #[test]
    fn steps_read_from_notes_and_labelled_with_their_cause() {
        assert_eq!(step_of("rc=101; cause=build; step=clippy").as_deref(), Some("clippy"));
        assert_eq!(step_of("rc=1;step=fmt").as_deref(), Some("fmt"));
        assert_eq!(step_of("rc=1; substep=fmt"), None, "a field, not a suffix");
        assert_eq!(step_of("rc=1; step="), None);
        assert_eq!(step_of("rc=1"), None);
        assert_eq!(step_of(&format!("rc=1; step={}", "s".repeat(200))).map(|s| s.len()), Some(64), "a hostile one is cut");
        assert_eq!(failure_label(Some("build"), Some("clippy")), "build/clippy");
        assert_eq!(failure_label(Some("test"), Some("test")), "test");
        assert_eq!(failure_label(Some("test"), None), "test");
        assert_eq!(failure_label(None, None), "unstated");
        assert_eq!(failure_label(None, Some("fmt")), "unstated/fmt");
    }

    #[test]
    fn hostile_and_empty_windows_never_panic() {
        let empty = whole(&[]);
        assert_eq!((empty.span_seconds, empty.active_seconds, empty.landed, empty.landed_per_active_hour), (0, 0, 0, None));
        assert!(empty.hours.is_empty() && empty.workers.is_empty());
        let wide = run(&overlapping(), Window { start: Some(Timestamp(i64::MIN)), end: Some(Timestamp(i64::MAX)) });
        assert_eq!((wide.active_seconds, wide.landed, wide.span_seconds), (90 * 60, 2, i64::MAX));
        assert_eq!(wide.hours.len(), 2);
        let backward = run(&overlapping(), Window { start: Some(at(10, 0)), end: Some(at(9, 0)) });
        assert_eq!((backward.span_seconds, backward.active_seconds), (0, 0));
        let edge = run(&overlapping(), Window { start: Some(Timestamp(i64::MAX)), end: None });
        assert_eq!(edge.active_seconds, 0);
    }

    #[test]
    fn a_long_run_keeps_its_last_hours() {
        let hours = i64::try_from(MAX_HOURS).unwrap() + 10;
        let mut worker = vec![start(0, 0, "worker-1", "worker")];
        worker.extend((0..=hours).flat_map(|h| [line(h, 0, r#""event":"beat""#), line(h, 14, r#""event":"beat""#), line(h, 28, r#""event":"beat""#)]));
        let stats = whole(&[file("mini-aa", worker)]);
        assert!(stats.hours_cut);
        assert_eq!(stats.hours.len(), MAX_HOURS);
        assert_eq!(stats.hours.last().map(|h| h.start), Some(at(hours, 0)));
        assert_eq!(stats.hours.last().map(|h| h.active_seconds), Some(28 * 60));
    }

    #[test]
    fn intervals() {
        let set = Intervals::of([(5, 10), (0, 3), (2, 4), (10, 12), (7, 7), (9, 1)]);
        assert_eq!(set.0, [(0, 4), (5, 12)]);
        assert_eq!(set.measure(), 11);
        assert_eq!(set.measure_in(3, 6), 2);
        assert_eq!(set.measure_in(20, 30), 0);
        let cut = Intervals::of([(1, 2), (3, 6), (11, 20)]);
        assert_eq!(set.subtract(&cut).0, [(0, 1), (2, 3), (6, 11)]);
        assert_eq!(set.intersect(&cut).0, [(1, 2), (3, 4), (5, 6), (11, 12)]);
        assert_eq!(set.subtract(&Intervals::default()), set);
        assert_eq!(Intervals::default().subtract(&set), Intervals::default());
        assert_eq!(median(vec![3, 1, 2]), Some(2));
        assert_eq!(median(vec![4, 1]), Some(2));
        assert_eq!(median(Vec::new()), None);
    }
}
