//! The factory's key moments (#33): the times a ticket moves on, so the factory can step from one
//! to the next, back and forth, rather than only play through time. A moment is one of:
//!
//! - **a claim**: a ticket's claimed stretch begins (a claim while it is held already is none);
//! - **work beginning**: its working stretch begins (a `work` while worked on already is none);
//! - **a submit**: it goes onto the dock, waiting for the merge;
//! - **a gate's pass or fail**: a gate run ends with `rc=0` or not (a run read from its merges and
//!   send-backs passes when it merged one); a run that never said how it went is none;
//! - **a send back**: the gate sends it back to its worker;
//! - **a merge onto main**: a merge commit on main (the ticket whose merge names its PR), or a
//!   ticket's merge when the repository has no main.
//!
//! Each is named as a person would say it: `#120 sent back by the gate · alder`, the worker last.
//! [`step`] finds the moment after or before a time, of the kinds asked for; the moments at one
//! second are one step. Pure: the same events, the same moments.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{MainMerge, Timestamp};

use crate::activity::Activity;
use crate::colours::{NO_TICKET, Rgb, ticket_colour};
use crate::factory::{GateRun, Outcome, gate_runs};
use crate::tickets::{Phase, Ticket, tickets};

/// What a key moment is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum MomentKind {
    Claim,
    /// Work on it began.
    Work,
    /// Submitted: onto the dock, waiting for the merge.
    Submit,
    GatePass,
    GateFail,
    Sendback,
    /// A merge onto main.
    Merge,
}

impl MomentKind {
    /// Every kind, in the order a ticket meets them.
    pub const ALL: [Self; 7] = [Self::Claim, Self::Work, Self::Submit, Self::GatePass, Self::GateFail, Self::Sendback, Self::Merge];

    /// How a person names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Work => "work begins",
            Self::Submit => "submit",
            Self::GatePass => "gate passes",
            Self::GateFail => "gate fails",
            Self::Sendback => "send back",
            Self::Merge => "merge onto main",
        }
    }
}

/// A key moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Moment {
    pub t: Timestamp,
    pub kind: MomentKind,
    /// The ticket it moves; none for a gate run naming none, or a merge onto main no ticket's
    /// merge names.
    pub issue: Option<u64>,
    pub pr: Option<u64>,
    /// The worker's name: who claimed, worked on or submitted it; for a send back, who had
    /// submitted it.
    pub who: Option<String>,
    /// `#120 sent back by the gate · alder`.
    pub text: String,
    /// The note its event carried: a send back's or a gate run's cause, a merge's subject.
    pub note: Option<String>,
    /// Its ticket's colour; [`NO_TICKET`] without one.
    pub colour: Rgb,
}

/// Which way to step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// The first moment after the time.
    #[default]
    Next,
    /// The last moment before it.
    Previous,
    /// The first moment of all.
    First,
    /// The last moment of all.
    Last,
}

/// Where a step lands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// The moment's time; none when there is no moment that way.
    pub t: Option<Timestamp>,
    /// The moments of the kinds asked for at that second, in the order a ticket meets them.
    pub moments: Vec<Moment>,
    /// What the step lands on, as a person says it: its first moment's text, `(+n)` for the
    /// others at that second; `no later merge onto main`, say, when there is none.
    pub text: String,
    /// Its place among the distinct seconds that hold such moments, from 1; and how many there
    /// are.
    pub index: Option<usize>,
    pub count: usize,
}

/// The name of the instance's worker, else the instance.
fn name_of(activity: &Activity, instance: &str) -> String {
    activity.agent(instance).map_or_else(|| instance.to_owned(), |a| a.name.clone())
}

/// `<text> · <who>`, or `<text>` without one.
fn with_who(text: String, who: Option<&String>) -> String {
    who.map_or_else(|| text.clone(), |who| format!("{text} · {who}"))
}

/// Every key moment of `activity` and the merges onto `main`, in time order (then the order a
/// ticket meets them, then by ticket).
#[must_use]
pub fn moments(activity: &Activity, merges: &[MainMerge]) -> Vec<Moment> {
    let tickets = tickets(activity);
    let mut list: Vec<Moment> = tickets.iter().flat_map(|ticket| ticket_moments(activity, ticket)).collect();
    list.extend(gate_runs(activity, Timestamp(i64::MAX)).iter().filter_map(gate_moment));
    if merges.is_empty() {
        list.extend(tickets.iter().filter_map(|ticket| ticket_merge(activity, ticket)));
    } else {
        list.extend(merges.iter().map(|merge| main_merge(activity, &tickets, merge)));
    }
    list.sort_by_key(|m| (m.t, m.kind, m.issue));
    list
}

/// A ticket's claims, work beginning, submits and send backs: where its stretches of each begin.
fn ticket_moments(activity: &Activity, ticket: &Ticket) -> Vec<Moment> {
    let n = ticket.issue;
    let mut list = Vec::new();
    for (i, stretch) in ticket.stretches.iter().enumerate() {
        let by = || Some(name_of(activity, &stretch.by));
        let (kind, text, who) = match stretch.phase {
            Phase::Claimed => (MomentKind::Claim, format!("#{n} claimed"), by()),
            // A rework's start is inferred, not logged (#83): no moment of its own.
            Phase::Working if stretch.rework => continue,
            // A logged `work` after a send-back (swarm-core#14) is the rework's start (#98).
            Phase::Working if i.checked_sub(1).and_then(|p| ticket.stretches.get(p)).is_some_and(|s| s.phase == Phase::SentBack) => {
                (MomentKind::Work, format!("#{n} rework began"), by())
            }
            Phase::Working => (MomentKind::Work, format!("#{n} work began"), by()),
            Phase::Waiting => {
                let pr = ticket.pr.map(|pr| format!(" (PR #{pr})")).unwrap_or_default();
                (MomentKind::Submit, format!("#{n} submitted{pr}"), by())
            }
            Phase::SentBack => {
                // Whoever had it before the gate: its last claim, work or submit.
                let before = ticket.stretches.get(..i).unwrap_or_default();
                let held = before.iter().rev().find(|s| matches!(s.phase, Phase::Claimed | Phase::Working | Phase::Waiting));
                (MomentKind::Sendback, format!("#{n} sent back by the gate"), held.map(|s| name_of(activity, &s.by)))
            }
            Phase::Stopped => continue,
        };
        let note = (kind == MomentKind::Sendback).then(|| stretch.note.clone()).flatten();
        let pr = if matches!(kind, MomentKind::Submit | MomentKind::Sendback) { ticket.pr } else { None };
        list.push(Moment { t: stretch.start, kind, issue: Some(n), pr, text: with_who(text, who.as_ref()), who, note, colour: ticket_colour(n) });
    }
    list
}

/// A ticket's merge, when the repository has no main.
fn ticket_merge(activity: &Activity, ticket: &Ticket) -> Option<Moment> {
    let merged = ticket.merged.as_ref()?;
    let pr = merged.pr.map(|pr| format!(" (PR #{pr})")).unwrap_or_default();
    let who = ticket.worker.as_deref().map(|w| name_of(activity, w));
    let text = with_who(format!("#{} merged{pr}", ticket.issue), who.as_ref());
    Some(Moment { t: merged.t, kind: MomentKind::Merge, issue: Some(ticket.issue), pr: merged.pr, who, text, note: None, colour: ticket_colour(ticket.issue) })
}

/// A merge onto main, with the ticket whose merge names its PR.
fn main_merge(activity: &Activity, tickets: &[Ticket], merge: &MainMerge) -> Moment {
    let ticket = merge.pr.and_then(|pr| tickets.iter().find(|tk| tk.merged.as_ref().is_some_and(|m| m.pr == Some(pr))));
    let who = ticket.and_then(|tk| tk.worker.as_deref()).map(|w| name_of(activity, w));
    let what = match (ticket, merge.pr) {
        (Some(tk), Some(pr)) => format!("#{} merged onto main (PR #{pr})", tk.issue),
        (_, Some(pr)) => format!("PR #{pr} merged onto main"),
        _ => format!("{} merged onto main", merge.commit.get(..7).unwrap_or(&merge.commit)),
    };
    let issue = ticket.map(|tk| tk.issue);
    Moment {
        t: merge.t,
        kind: MomentKind::Merge,
        issue,
        pr: merge.pr,
        text: with_who(what, who.as_ref()),
        who,
        note: Some(merge.subject.clone()),
        colour: issue.map_or(NO_TICKET, ticket_colour),
    }
}

/// A gate run's end, when it said how it went.
fn gate_moment(run: &GateRun) -> Option<Moment> {
    let end = run.end?;
    let kind = match run.outcome {
        Outcome::Passed => MomentKind::GatePass,
        Outcome::Failed => MomentKind::GateFail,
        Outcome::Running | Outcome::Unknown => return None,
    };
    let verb = if kind == MomentKind::GatePass { "passed" } else { "failed" };
    let cause = run.cause.as_ref().map(|c| format!(": {c}")).unwrap_or_default();
    let named: Vec<String> =
        if run.issues.is_empty() { run.refs.iter().map(|pr| format!("PR #{pr}")).collect() } else { run.issues.iter().map(|n| format!("#{n}")).collect() };
    let what = if named.is_empty() { String::new() } else { format!(" · {}", named.join(", ")) };
    let issue = run.issues.first().copied();
    Some(Moment {
        t: end,
        kind,
        issue,
        pr: run.refs.first().copied(),
        who: None,
        text: format!("{} {verb}{cause}{what}", run.sub),
        note: run.cause.clone(),
        colour: issue.map_or(NO_TICKET, ticket_colour),
    })
}

/// The kinds a step takes: every kind when none are named.
fn takes(kinds: &[MomentKind], kind: MomentKind) -> bool {
    kinds.is_empty() || kinds.contains(&kind)
}

/// The step from `from` (none: before the first moment for [`Direction::Next`], after the last
/// for [`Direction::Previous`]) in `direction`, over the `moments` of `kinds` (none named: every
/// kind). The moments at one second are one step.
#[must_use]
pub fn step(moments: &[Moment], kinds: &[MomentKind], from: Option<Timestamp>, direction: Direction) -> Step {
    let taken: Vec<&Moment> = moments.iter().filter(|m| takes(kinds, m.kind)).collect();
    let mut seconds: Vec<Timestamp> = taken.iter().map(|m| m.t).collect();
    seconds.dedup();
    let count = seconds.len();
    let index = match direction {
        Direction::First => (count > 0).then_some(0),
        Direction::Last => count.checked_sub(1),
        Direction::Next => match from {
            None => (count > 0).then_some(0),
            Some(from) => Some(seconds.partition_point(|t| *t <= from)).filter(|i| *i < count),
        },
        Direction::Previous => match from {
            None => count.checked_sub(1),
            Some(from) => seconds.partition_point(|t| *t < from).checked_sub(1),
        },
    };
    let Some((index, t)) = index.and_then(|i| seconds.get(i).map(|t| (i, *t))) else {
        let what = match kinds {
            [one] => one.name(),
            _ => "moment",
        };
        let way = if matches!(direction, Direction::Previous | Direction::First) { "earlier" } else { "later" };
        return Step { t: None, moments: Vec::new(), text: format!("no {way} {what}"), index: None, count };
    };
    let at: Vec<Moment> = taken.into_iter().filter(|m| m.t == t).cloned().collect();
    let text = match at.as_slice() {
        [] => String::new(),
        [one] => one.text.clone(),
        [first, rest @ ..] => format!("{} (+{})", first.text, rest.len()),
    };
    Step { t: Some(t), moments: at, text, index: Some(index + 1), count }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::tests::sample;
    use crate::factory::tests::{FAILING, GATES};
    use swarmception_model::{parse_file, parse_merge};

    fn at(text: &str) -> Timestamp {
        Timestamp::parse(text).unwrap()
    }

    fn failing() -> Activity {
        Activity::of(&[parse_file("mini-w1", FAILING), parse_file("mini-c1", GATES)])
    }

    #[test]
    fn a_tickets_key_moments_in_order_named_as_a_person_would() {
        let list = moments(&failing(), &[]);
        let texts: Vec<(String, MomentKind, &str)> = list.iter().map(|m| (m.t.to_string(), m.kind, m.text.as_str())).collect();
        assert_eq!(
            texts,
            [
                ("2026-10-31T01:00:00Z".to_owned(), MomentKind::Claim, "#5 claimed · worker-1"),
                ("2026-10-31T01:00:05Z".to_owned(), MomentKind::Work, "#5 work began · worker-1"),
                ("2026-10-31T01:10:00Z".to_owned(), MomentKind::Submit, "#5 submitted (PR #9) · worker-1"),
                ("2026-10-31T01:19:00Z".to_owned(), MomentKind::GateFail, "gate-9 failed: build · #5"),
                ("2026-10-31T01:20:00Z".to_owned(), MomentKind::Sendback, "#5 sent back by the gate · worker-1"),
                ("2026-10-31T01:30:00Z".to_owned(), MomentKind::Work, "#5 rework began · worker-1"),
                ("2026-10-31T01:40:00Z".to_owned(), MomentKind::Submit, "#5 submitted (PR #9) · worker-1"),
                ("2026-10-31T02:00:00Z".to_owned(), MomentKind::GatePass, "gate-pr9b passed · #5"),
                ("2026-10-31T02:00:00Z".to_owned(), MomentKind::Merge, "#5 merged (PR #9) · worker-1"),
            ]
        );
        let back = list.iter().find(|m| m.kind == MomentKind::Sendback).unwrap();
        assert_eq!((back.note.as_deref(), back.issue, back.pr, back.colour), (Some("build: the API changed"), Some(5), Some(9), ticket_colour(5)));
        assert_eq!(list[3].note.as_deref(), Some("build"), "the gate's cause");
    }

    #[test]
    fn merges_onto_main_are_the_merge_moments_when_there_is_a_main() {
        let merges: Vec<MainMerge> = [
            "be043bc0\u{1f}1793420113\u{1f}Merge pull request #173 from mornycorp/170-spec",
            "15bad2c0\u{1f}1793420255\u{1f}Merge pull request #174 from mornycorp/progress-7",
        ]
        .into_iter()
        .filter_map(parse_merge)
        .collect();
        let list = moments(&Activity::of(&sample()), &merges);
        let merged: Vec<(&str, Option<u64>, Rgb)> = list.iter().filter(|m| m.kind == MomentKind::Merge).map(|m| (m.text.as_str(), m.issue, m.colour)).collect();
        assert_eq!(merged, [("#170 merged onto main (PR #173) · worker-2", Some(170), ticket_colour(170)), ("PR #174 merged onto main", None, NO_TICKET)]);
        // #168 was only worked on (no claim), its worker's session ended by the coordinator.
        let first: Vec<&str> = list.iter().take(2).map(|m| m.text.as_str()).collect();
        assert_eq!(first, ["#168 work began · worker-1", "#170 claimed · worker-2"]);
    }

    #[test]
    fn stepping_forth_and_back_by_kind() {
        let list = moments(&failing(), &[]);
        let next = |from: &str, kinds: &[MomentKind]| step(&list, kinds, Some(at(from)), Direction::Next);
        let previous = |from: &str, kinds: &[MomentKind]| step(&list, kinds, Some(at(from)), Direction::Previous);
        let s = next("2026-10-31T01:05:00Z", &[]);
        assert_eq!((s.t, s.text.as_str(), s.index, s.count), (Some(at("2026-10-31T01:10:00Z")), "#5 submitted (PR #9) · worker-1", Some(3), 8));
        assert_eq!(next("2026-10-31T01:10:00Z", &[]).t, Some(at("2026-10-31T01:19:00Z")), "strictly after");
        assert_eq!(previous("2026-10-31T01:10:00Z", &[]).t, Some(at("2026-10-31T01:00:05Z")), "strictly before");
        // Two at one second are one step, named first and counted.
        let both = next("2026-10-31T01:45:00Z", &[]);
        assert_eq!((both.moments.len(), both.text.as_str()), (2, "gate-pr9b passed · #5 (+1)"));
        // Only merges; only gate runs.
        let merge = next("2026-10-31T01:00:00Z", &[MomentKind::Merge]);
        assert_eq!((merge.t, merge.index, merge.count), (Some(at("2026-10-31T02:00:00Z")), Some(1), 1));
        assert_eq!(merge.text, "#5 merged (PR #9) · worker-1", "only the kinds asked for");
        let none = next("2026-10-31T02:00:00Z", &[MomentKind::Merge]);
        assert_eq!((none.t, none.text.as_str(), none.index), (None, "no later merge onto main", None));
        assert_eq!(previous("2026-10-31T01:00:00Z", &[]).text, "no earlier moment");
        let gates = previous("2026-10-31T03:00:00Z", &[MomentKind::GatePass, MomentKind::GateFail]);
        assert_eq!(gates.text, "gate-pr9b passed · #5");
        // From nowhere, and the ends.
        assert_eq!(step(&list, &[], None, Direction::Next).t, Some(at("2026-10-31T01:00:00Z")));
        assert_eq!(step(&list, &[], None, Direction::Previous).t, Some(at("2026-10-31T02:00:00Z")));
        assert_eq!(step(&list, &[], Some(at("2026-10-31T01:30:00Z")), Direction::First).index, Some(1));
        assert_eq!(step(&list, &[], Some(at("2026-10-31T01:30:00Z")), Direction::Last).index, Some(8));
        assert_eq!(step(&[], &[], None, Direction::Next), Step { text: "no later moment".to_owned(), ..Step::default() });
        // Far out of range: answers, never a panic.
        assert_eq!(step(&list, &[], Some(Timestamp(i64::MIN)), Direction::Next).index, Some(1));
        assert_eq!(step(&list, &[], Some(Timestamp(i64::MAX)), Direction::Previous).index, Some(8));
    }

    #[test]
    fn a_gate_run_that_never_said_how_it_went_is_no_moment() {
        let gates = r#"{"event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T01:00:00Z"}
{"event":"sub.start","name":"gate","note":"local gate: #9 (#5)","sub":"gate-9","t":"2026-10-31T01:15:00Z"}
{"event":"sub.stop","sub":"gate-9","t":"2026-10-31T01:19:00Z"}
{"event":"sub.start","name":"gate","note":"local gate: #9 (#5)","sub":"gate-10","t":"2026-10-31T01:20:00Z"}"#;
        let list = moments(&Activity::of(&[parse_file("mini-c1", gates)]), &[]);
        assert!(list.is_empty(), "{list:?}");
    }
}
