//! Send-backs as the tracking format defines them (Swarm Core's docs/tracking-format.md ›
//! Send-backs; #106): a gate that fails for the change's cause sends each PR it gated back, with
//! or without a `sendback` event. Until #106 only the event read as one, so a ticket whose gate
//! failed showed as waiting until its worker's `prwatch` logged the rework's `work`. Swarm Core's
//! `prwatch::sendback` (pinned) reads the same rule, and a test here checks the two agree.
//!
//! - **A gate** is a coordinator's helper whose `sub` begins `gate`. **What it gated**: every
//!   `#<n>` its start note names before its first `;` (after any `rerun of <sub>:`), a PR or a
//!   ticket's issue; a rerun that names none gated those of the gate it reruns.
//! - **It failed for the change's cause** when its `sub.stop` note's first field is `rc=<n>`, `n`
//!   not 0, and its `cause=` isn't one of the machine's ([`TRANSIENT`]: a rerun of the same tree
//!   passes). No `rc` (stopped, superseded, never finished) is no failure.
//! - **Unless a later gate reruns it**, saying so (`rerun of <sub>:`): the rerun's result counts
//!   instead. A gate that only gates the same PRs again is no rerun by this rule.
//! - **Each ticket it gated is sent back once, at the gate's stop**, unless it merged since it was
//!   last claimed or submitted (a PR a note names in passing, or one the gate merged before it
//!   failed). A `sendback` event for the ticket after that failure and before its next gate is
//!   the same send-back: its note (the coordinator's words) replaces the gate's result, and it
//!   counts once. So is one the failing gate logged itself (its `sub`) before its stop: it stands
//!   alone, at its own time (swarm-core's `prwatch` reads the stop's time then, a minute or so
//!   later; it only ever keeps the latest).
//!
//! [`told`] adds each such send-back to the activity's issue events as a `sendback` at the gate's
//! stop, logged by the gate's coordinator with the gate's `sub`, and drops the `sendback` events
//! it answers, so the tickets, the timeline, the details, the factory and `stats.get` all read
//! one list. Pure: no I/O, no clock.

use std::collections::{BTreeMap, BTreeSet};

use swarmception_model::{Kind, Timestamp};

use crate::activity::{Agent, IssueEvent};

/// The causes that are the machine's, not the change's (Swarm Core's `gate::Cause::transient`).
pub const TRANSIENT: [&str; 5] = ["memory", "link", "toolchain", "git-temp", "gpu"];

/// Whether `cause` (a stop note's `cause=`) is the machine's: [`TRANSIENT`].
#[must_use]
pub fn transient(cause: &str) -> bool {
    TRANSIENT.contains(&cause)
}

/// A stop note's fields: split at `;` and whitespace, none empty (Swarm Core's `gate::tokens`).
fn tokens(note: &str) -> impl DoubleEndedIterator<Item = &str> {
    note.split(|c: char| c == ';' || c.is_whitespace()).filter(|t| !t.is_empty())
}

/// Why a gate's stop note says it failed for the change's cause, as a send-back's note
/// (`cause=<cause>: <sub> failed (rc=<n>)`), or none: it passed, it stopped with no `rc`, or its
/// cause is the machine's.
#[must_use]
pub fn failure(sub: &str, note: &str) -> Option<String> {
    let rc: i64 = tokens(note).next()?.strip_prefix("rc=")?.parse().ok()?;
    let cause = tokens(note).rev().find_map(|f| f.strip_prefix("cause=")).filter(|c| !c.is_empty());
    if rc == 0 || cause.is_some_and(transient) {
        return None;
    }
    Some(match cause {
        Some(cause) => format!("cause={cause}: {sub} failed (rc={rc})"),
        None => format!("{sub} failed (rc={rc})"),
    })
}

/// The gate a start note reruns (`rerun of <sub>:`), and every `#<n>` it names before its first
/// `;`, each once (Swarm Core's `prwatch::sendback`).
#[must_use]
pub fn gated(note: &str) -> (Option<String>, Vec<u64>) {
    let what = note.split(';').next().unwrap_or_default();
    let (rerun_of, what) = match what.strip_prefix("rerun of ").and_then(|r| r.split_once(':')) {
        Some((of, rest)) => (Some(of.trim().to_owned()), rest),
        None => (None, what),
    };
    let mut refs = Vec::new();
    for part in what.split('#').skip(1) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse::<u64>()
            && !refs.contains(&n)
        {
            refs.push(n);
        }
    }
    (rerun_of, refs)
}

/// One gate as its coordinator's file logs it.
struct Gate<'a> {
    instance: &'a str,
    sub: &'a str,
    start: Option<Timestamp>,
    rerun_of: Option<String>,
    refs: Vec<u64>,
    /// Its stop's time and its failure, when it failed for the change's cause.
    failed: Option<(Timestamp, String)>,
}

/// Each agent's gates, a rerun naming nothing given what the gate it reruns gated, and the
/// failures a rerun answers taken out.
fn gates(agents: &[Agent]) -> Vec<Gate<'_>> {
    let mut all = Vec::new();
    for agent in agents {
        let mut own: Vec<Gate<'_>> = agent
            .subs
            .iter()
            .filter(|s| s.sub.starts_with("gate"))
            .map(|s| {
                let (rerun_of, refs) = s.start_note.as_deref().map(gated).unwrap_or_default();
                let failed = s.stop_note.as_deref().filter(|_| s.unfinished.is_none()).and_then(|n| failure(&s.sub, n)).map(|why| (s.span.end, why));
                Gate { instance: &agent.instance, sub: &s.sub, start: s.start_note.is_some().then_some(s.span.start), rerun_of, refs, failed }
            })
            .collect();
        // Reruns of reruns: a few rounds settle any chain a coordinator logs.
        for _ in 0..4 {
            let inherited: Vec<(usize, Vec<u64>)> = own
                .iter()
                .enumerate()
                .filter(|(_, g)| g.refs.is_empty())
                .filter_map(|(i, g)| {
                    let of = g.rerun_of.as_deref()?;
                    Some((i, own.iter().find(|o| o.sub == of && !o.refs.is_empty())?.refs.clone()))
                })
                .collect();
            for (i, refs) in inherited {
                if let Some(g) = own.get_mut(i) {
                    g.refs = refs;
                }
            }
        }
        let rerun: BTreeSet<String> = own.iter().filter_map(|g| g.rerun_of.clone()).collect();
        for g in &mut own {
            if rerun.contains(g.sub) {
                g.failed = None;
            }
        }
        all.extend(own);
    }
    all
}

/// What happened to a ticket at a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    /// A gate that named it started.
    Gated,
    /// A gate that named it failed for the change's cause: the gate, by its index.
    Failed(usize),
    /// A `sendback` event named it: the event, by its index.
    SentBack(usize),
}

/// The issue each number in a gate's note stands for: a PR the events name is its issue's, else
/// an issue the events name is itself; none for a number nothing names.
fn issue_of(n: u64, prs: &BTreeMap<u64, u64>, issues: &BTreeSet<u64>) -> Option<u64> {
    prs.get(&n).copied().or_else(|| issues.contains(&n).then_some(n))
}

/// Whether `issue` merged since it was last claimed or submitted, by `t`: a gate's failure after
/// that sends nothing back.
fn merged_by(events: &[IssueEvent], issue: u64, t: Timestamp) -> bool {
    let latest = |pick: fn(&Kind) -> bool| events.iter().filter(|e| e.issue == issue && e.t <= t && pick(&e.kind)).map(|e| e.t).max();
    let merged = latest(|k| matches!(k, Kind::Merge { .. }));
    let handed_in = latest(|k| matches!(k, Kind::Claim { .. } | Kind::Submit { .. }));
    merged.is_some_and(|m| handed_in.is_none_or(|h| m >= h))
}

/// The pull request `issue`'s events name last by `t`.
fn pr_by(events: &[IssueEvent], issue: u64, t: Timestamp) -> Option<u64> {
    events.iter().rev().filter(|e| e.issue == issue && e.t <= t).find_map(|e| e.kind.mr())
}

/// Each ticket's steps, in time order: at a tie a gate's start before a failure before an event.
fn steps(gates: &[Gate<'_>], events: &[IssueEvent]) -> BTreeMap<u64, Vec<(Timestamp, Step)>> {
    let prs: BTreeMap<u64, u64> = events.iter().filter_map(|e| Some((e.kind.mr()?, e.issue))).collect();
    let issues: BTreeSet<u64> = events.iter().map(|e| e.issue).collect();
    let mut steps: BTreeMap<u64, Vec<(Timestamp, Step)>> = BTreeMap::new();
    for (i, gate) in gates.iter().enumerate() {
        let named: BTreeSet<u64> = gate.refs.iter().filter_map(|&n| issue_of(n, &prs, &issues)).collect();
        for issue in named {
            let list = steps.entry(issue).or_default();
            list.extend(gate.start.map(|t| (t, Step::Gated)));
            list.extend(gate.failed.as_ref().map(|(t, _)| (*t, Step::Failed(i))));
        }
    }
    for (i, event) in events.iter().enumerate().filter(|(_, e)| matches!(e.kind, Kind::Sendback { .. })) {
        steps.entry(event.issue).or_default().push((event.t, Step::SentBack(i)));
    }
    for list in steps.values_mut() {
        list.sort();
    }
    steps
}

/// The send-back gate `g`'s failure makes of `issue` at `t`: none when the ticket merged since
/// it was last handed in.
fn sent_back(gates: &[Gate<'_>], g: usize, events: &[IssueEvent], issue: u64, t: Timestamp) -> Option<IssueEvent> {
    let gate = gates.get(g)?;
    let why = gate.failed.as_ref()?.1.clone();
    if merged_by(events, issue, t) {
        return None;
    }
    let mr = pr_by(events, issue, t);
    Some(IssueEvent { t, instance: gate.instance.to_owned(), issue, kind: Kind::Sendback { issue, mr }, sub: Some(gate.sub.to_owned()), note: Some(why) })
}

/// The issue events (`events`, oldest first) with each send-back a gate's failure makes added as
/// a `sendback` at its stop, and the `sendback` events those answer dropped: the module's rule.
#[must_use]
pub fn told(agents: &[Agent], events: Vec<IssueEvent>) -> Vec<IssueEvent> {
    let gates = gates(agents);
    if gates.iter().all(|g| g.failed.is_none()) {
        return events;
    }
    let mut added: Vec<IssueEvent> = Vec::new();
    let mut answered: BTreeSet<usize> = BTreeSet::new();
    for (issue, list) in steps(&gates, &events) {
        // The send-back a gate's failure made, not yet followed by the ticket's next gate; and the
        // gates whose own `sendback`s came since the ticket's last gate began.
        let mut open: Option<usize> = None;
        let mut logged: Vec<&str> = Vec::new();
        for (t, step) in list {
            match step {
                Step::Gated => (open, logged) = (None, Vec::new()),
                // A gate that sent the ticket back itself before its stop: that `sendback` is it.
                Step::Failed(g) if gates.get(g).is_some_and(|gate| logged.contains(&gate.sub)) => open = None,
                Step::Failed(g) => {
                    open = sent_back(&gates, g, &events, issue, t).map(|back| {
                        added.push(back);
                        added.len() - 1
                    });
                }
                Step::SentBack(e) => {
                    let Some(same) = open.take().and_then(|a| added.get_mut(a)) else {
                        logged.extend(events.get(e).and_then(|e| e.sub.as_deref()));
                        continue;
                    };
                    // The same send-back as the gate's failure: the coordinator's words say why.
                    if let Some(note) = events.get(e).and_then(|e| e.note.clone()).filter(|n| !n.is_empty()) {
                        same.note = Some(note);
                    }
                    answered.insert(e);
                }
            }
        }
    }
    let mut told: Vec<IssueEvent> = events.into_iter().enumerate().filter(|(i, _)| !answered.contains(i)).map(|(_, e)| e).collect();
    told.extend(added);
    told.sort_by_key(|e| e.t);
    told
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use swarmception_model::Kind;

    use super::*;
    use crate::activity::Activity;
    use crate::tickets::tests::files;

    /// A worker's #7 on PR #9, submitted at 11:00.
    const WORKER: &str = r#"{"event":"session.start","machine":"pc","name":"worker-1","role":"worker","t":"2026-10-17T09:59:00Z"}
{"event":"claim","issue":7,"mr":9,"t":"2026-10-17T10:00:00Z"}
{"event":"work","issue":7,"t":"2026-10-17T10:00:05Z"}
{"event":"submit","issue":7,"mr":9,"t":"2026-10-17T11:00:00Z"}"#;

    const START: &str = r#"{"event":"session.start","machine":"pc","name":"coordinator","role":"coordinator","t":"2026-10-17T09:00:00Z"}"#;

    fn gate(sub: &str, note: &str, from: &str, stop: &str, to: &str) -> String {
        format!(
            "{{\"event\":\"sub.start\",\"name\":\"gate\",\"note\":\"{note}\",\"sub\":\"{sub}\",\"t\":\"2026-10-17T{from}Z\"}}\n{{\"event\":\"sub.stop\",\"note\":\"{stop}\",\"sub\":\"{sub}\",\"t\":\"2026-10-17T{to}Z\"}}"
        )
    }

    /// A send-back as the tests read it: its time, issue, PR, gate and note.
    type Told = (String, u64, Option<u64>, Option<String>, Option<String>);

    fn sendbacks(coordinator: &str) -> Vec<Told> {
        let activity = Activity::of(&files(&[("pc-c", coordinator), ("pc-w", WORKER)]));
        activity
            .issue_events
            .iter()
            .filter(|e| matches!(e.kind, Kind::Sendback { .. }))
            .map(|e| (e.t.to_string(), e.issue, e.kind.mr(), e.sub.clone(), e.note.clone()))
            .collect()
    }

    #[test]
    fn a_gate_that_fails_for_the_change_sends_its_tickets_back_at_its_stop() {
        let coordinator = format!(
            "{START}\n{}",
            gate("gate-mr9", "local gate: #9 (#7); capacity=1 free=9.8; kind=smoke", "12:00:00", "rc=101; cause=test; kind=smoke", "12:10:00")
        );
        assert_eq!(
            sendbacks(&coordinator),
            [("2026-10-17T12:10:00Z".to_owned(), 7, Some(9), Some("gate-mr9".to_owned()), Some("cause=test: gate-mr9 failed (rc=101)".to_owned()))],
            "the PR and its issue both named: one send-back, at the stop, with no sendback event"
        );
        // The coordinator's own sendback after it is the same send-back, in its words, once.
        let with_event = format!(
            "{coordinator}\n{}",
            r#"{"event":"sendback","issue":7,"mr":9,"note":"cause=test: the tests fail","sub":"gate-mr9","t":"2026-10-17T12:11:00Z"}"#
        );
        assert_eq!(
            sendbacks(&with_event),
            [("2026-10-17T12:10:00Z".to_owned(), 7, Some(9), Some("gate-mr9".to_owned()), Some("cause=test: the tests fail".to_owned()))]
        );
        // One the gate logged itself before its stop is the send-back, at its own time.
        let logged_first = format!(
            "{START}\n{}\n{}\n{}",
            r#"{"event":"sub.start","name":"gate","note":"local gate: #9 (#7)","sub":"gate-mr9","t":"2026-10-17T12:00:00Z"}"#,
            r#"{"event":"sendback","issue":7,"mr":9,"note":"cause=test: a test failed","sub":"gate-mr9","t":"2026-10-17T12:09:00Z"}"#,
            r#"{"event":"sub.stop","note":"rc=1; cause=test","sub":"gate-mr9","t":"2026-10-17T12:10:00Z"}"#
        );
        assert_eq!(
            sendbacks(&logged_first),
            [("2026-10-17T12:09:00Z".to_owned(), 7, Some(9), Some("gate-mr9".to_owned()), Some("cause=test: a test failed".to_owned()))]
        );
        // One after the ticket's next gate is a send-back of its own.
        let regated = format!(
            "{coordinator}\n{}\n{}",
            gate("gate-mr9b", "local gate: #9; kind=smoke", "12:20:00", "superseded: main moved", "12:21:00"),
            r#"{"event":"sendback","issue":7,"mr":9,"note":"cause=conflict: lib.rs","t":"2026-10-17T12:30:00Z"}"#
        );
        assert_eq!(sendbacks(&regated).iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["2026-10-17T12:10:00Z", "2026-10-17T12:30:00Z"]);
    }

    /// #106: a gate's failure with no `sendback` logged reads as sent back from its stop, the same
    /// in the ticket's life, the details, the factory and `stats.get`.
    #[test]
    fn the_ticket_the_details_the_factory_and_the_stats_agree() {
        use swarmception_model::{Issues, Timestamp};

        use crate::details::{Subject, details};
        use crate::factory::{BenchStatus, CrateState, scene};
        use crate::stats::{Window, stats};
        use crate::tickets::{Phase, tickets};

        let coordinator = format!("{START}\n{}", gate("gate-mr9", "local gate: #9 (#7); kind=smoke", "12:00:00", "rc=1; cause=build; kind=smoke", "12:10:00"));
        let activity = Activity::of(&files(&[("pc-c", coordinator.as_str()), ("pc-w", WORKER)]));
        let at = |t: &str| Timestamp::parse(&format!("2026-10-17T{t}Z")).unwrap();

        let all = tickets(&activity);
        let back = all[0].stretches.last().unwrap();
        assert_eq!((back.phase, back.start, back.open), (Phase::SentBack, at("12:10:00"), true), "sent back from the gate's stop, not waiting");
        assert_eq!(back.note.as_deref(), Some("cause=build: gate-mr9 failed (rc=1)"));

        let told = details(&activity, &[], &Issues::default(), &Subject::Ticket { issue: 7 }).unwrap();
        assert!(told.facts.iter().any(|f| f.label == "sent back" && f.value == "1×"), "{:?}", told.facts);
        assert!(told.entries.iter().any(|e| e.what == "sent back" && e.t == at("12:10:00")), "{:?}", told.entries);

        let factory = scene(&activity, &[], at("12:15:00"), 300);
        let bench = factory.benches.iter().find(|b| b.key.ends_with("worker-1")).unwrap();
        assert_eq!((bench.status, bench.crates.first().map(|c| c.state)), (BenchStatus::SentBack, Some(CrateState::SentBack)));

        assert_eq!(stats(&activity, &all, &[], Window::default()).sendbacks, 1);
    }

    #[test]
    fn a_pass_a_machines_failure_a_stopped_gate_or_an_answered_rerun_sends_nothing_back() {
        for stop in ["rc=0; merged #9", "rc=101; cause=memory", "rc=1; cause=link; kind=smoke", "superseded: main moved", ""] {
            let coordinator = format!("{START}\n{}", gate("gate-mr9", "local gate: #9 (#7)", "12:00:00", stop, "12:10:00"));
            assert!(sendbacks(&coordinator).is_empty(), "{stop:?}");
        }
        let failed = gate("gate-mr9", "local gate: #9 (#7)", "12:00:00", "rc=101; cause=test", "12:10:00");
        let rerun =
            |stop: &str| format!("{START}\n{failed}\n{}", gate("gate-mr9-rerun", "rerun of gate-mr9: the failed tests alone", "12:11:00", stop, "12:15:00"));
        assert!(sendbacks(&rerun("rc=0; merged #9")).is_empty(), "a flake the rerun answers");
        assert_eq!(
            sendbacks(&rerun("rc=101; cause=test")).iter().map(|s| (s.0.as_str(), s.3.as_deref())).collect::<Vec<_>>(),
            [("2026-10-17T12:15:00Z", Some("gate-mr9-rerun"))],
            "the rerun names no PR: it gated the gate's, and its failure is the send-back"
        );
        // A gate that never logged its stop has no result.
        let unfinished = format!("{START}\n{}", r#"{"event":"sub.start","name":"gate","note":"local gate: #9","sub":"gate-mr9","t":"2026-10-17T12:00:00Z"}"#);
        assert!(sendbacks(&unfinished).is_empty());
    }

    #[test]
    fn a_ticket_merged_before_the_failure_or_a_number_nothing_names_is_not_sent_back() {
        let merged = format!(
            "{START}\n{}\n{}",
            r#"{"event":"sub.start","name":"gate","note":"local gate: #9 (#7), #12 (with #300); kind=smoke","sub":"gate-a","t":"2026-10-17T12:00:00Z"}"#,
            r#"{"event":"merge","issue":7,"mr":9,"sub":"gate-a","t":"2026-10-17T12:05:00Z"}
{"event":"sub.stop","note":"rc=1; cause=build","sub":"gate-a","t":"2026-10-17T12:10:00Z"}"#
        );
        assert!(sendbacks(&merged).is_empty(), "#7 merged before the gate failed; #12 and #300 name nothing");
    }

    #[test]
    fn the_rules_pieces() {
        assert_eq!(failure("gate-1", "rc=101; cause=test; kind=smoke").as_deref(), Some("cause=test: gate-1 failed (rc=101)"));
        assert_eq!(failure("gate-1", "rc=1").as_deref(), Some("gate-1 failed (rc=1)"));
        assert_eq!(failure("gate-1", "kind=smoke; rc=1"), None, "rc must come first");
        assert_eq!(failure("gate-1", "rc=1; cause=git-temp"), None);
        assert_eq!(gated("rerun of gate-1: local gate: #12 #13 (#4 #5); kind=smoke"), (Some("gate-1".into()), vec![12, 13, 4, 5]));
        assert_eq!(gated("full gate of main; capacity=1; kind=full; commit=abc"), (None, vec![]));
    }

    /// Swarm Core's pinned `prwatch::sendback` reads the same files the same way: the latest
    /// send-back of each PR at the same time, and the same causes are the machine's.
    #[test]
    fn swarm_cores_prwatch_reads_the_same_send_backs() {
        use swarm_core::gate::Cause;
        use swarm_core::prwatch::sendback::send_backs;

        for cause in Cause::ALL {
            assert_eq!(transient(cause.name()), cause.transient(), "{}", cause.name());
        }
        let failed =
            gate("gate-mr9", "local gate: #9 (#7), #10 (#8); capacity=1 free=9.8; kind=smoke", "12:00:00", "rc=101; cause=test; kind=smoke", "12:10:00");
        let worker_8 = r#"{"event":"session.start","machine":"pc","name":"worker-2","role":"worker","t":"2026-10-17T09:59:00Z"}
{"event":"claim","issue":8,"mr":10,"t":"2026-10-17T10:00:00Z"}
{"event":"submit","issue":8,"mr":10,"t":"2026-10-17T11:30:00Z"}"#;
        let cases = [
            format!("{START}\n{failed}"),
            format!(
                "{START}\n{failed}\n{}",
                r#"{"event":"sendback","issue":7,"mr":9,"note":"cause=test: fix it","sub":"gate-mr9","t":"2026-10-17T12:11:00Z"}"#
            ),
            format!(
                "{START}\n{failed}\n{}",
                gate("gate-mr9-rerun", "rerun of gate-mr9: the failed tests alone", "12:11:00", "rc=0; merged #9 #10", "12:15:00")
            ),
            format!("{START}\n{failed}\n{}", gate("gate-mr9-rerun", "rerun of gate-mr9: the failed tests alone", "12:11:00", "rc=101; cause=test", "12:15:00")),
            format!("{START}\n{}", gate("gate-mr9", "local gate: #9 (#7); kind=smoke", "12:00:00", "rc=101; cause=memory", "12:10:00")),
            format!(
                "{START}\n{failed}\n{}\n{}",
                gate("gate-mr10", "local gate: #10 (#8)", "12:20:00", "rc=1; cause=build", "12:22:00"),
                r#"{"event":"sendback","issue":7,"mr":9,"note":"cause=conflict: lib.rs","t":"2026-10-17T12:40:00Z"}"#
            ),
        ];
        for coordinator in &cases {
            let theirs = send_backs(&[("pc-c".to_owned(), coordinator.clone()), ("pc-w".to_owned(), WORKER.to_owned())], &BTreeSet::from([9, 10]));
            let theirs: BTreeMap<u64, String> = theirs.into_iter().map(|(mr, b)| (mr, b.t)).collect();
            let activity = Activity::of(&files(&[("pc-c", coordinator.as_str()), ("pc-w", WORKER), ("pc-w2", worker_8)]));
            let mut ours: BTreeMap<u64, String> = BTreeMap::new();
            for event in activity.issue_events.iter().filter(|e| matches!(e.kind, Kind::Sendback { .. })) {
                if let Some(mr) = event.kind.mr() {
                    ours.insert(mr, event.t.to_string());
                }
            }
            assert_eq!(ours, theirs, "{coordinator}");
        }
    }
}
