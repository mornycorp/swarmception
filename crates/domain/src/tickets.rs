//! Each ticket's life, from its issue events (Swarm Core's docs/tracking-format.md): when it was
//! claimed, when work on it began, when the work finished (its `submit`), when it merged, and the
//! stretches between. A ticket's events are read in time order across every agent's file:
//!
//! - `claim` starts a *claimed* stretch, unless the ticket is already claimed, worked on or waiting;
//! - `work` starts *working*; `submit` starts *waiting* for the merge;
//! - `sendback` starts *sent back*, until work begins again. A worker answers a send-back by
//!   fixing its PR and submitting again, and logs no `work` before it (Swarm Core's `ready` logs
//!   the `submit` alone), so a `submit` straight after a send-back is its rework (#83): the
//!   send-back reads as such for [`SENT_BACK_MARK`] seconds (or until the submit, if sooner),
//!   and the time from there to the submit is *working* again, by the submitter, a
//!   [`Stretch::rework`]. The ticket's second run is drawn as its first, in its own colour; a
//!   send-back nothing has answered yet stays *sent back*;
//! - `stop` but `done` starts *stopped* (a handoff, a block, a pause);
//! - `stop done` while claimed or worked on ends the stretch and marks the ticket done without a
//!   merge (its output was something else, such as new issues); after a `submit` it changes
//!   nothing, as the ticket waits for its merge;
//! - `release` ends the stretch, and `merge` ends it and marks the ticket merged;
//! - a *claimed*, *working* or *sent back* stretch ends where its holder's session ended (#28),
//!   and the ticket reads as *abandoned*: the holder is the instance that last claimed, worked
//!   on or submitted it, and its session the one going when it did (its last before, when that
//!   had ended already: sent back after its worker left, it is abandoned at the send-back). A
//!   session ends at its `session.stop` (a `cargo xtask finish` logs one), a coordinator's
//!   `session.stop --of` it, or its next `session.start`. A session that never logged a stop is
//!   still going, as the tracking format says of a dead agent's until the coordinator ends it,
//!   and as the statistics' *on shift* and the factory's actors read it: it ends nothing. A
//!   *waiting* stretch waits for the coordinator, and a *stopped* one was set down on purpose,
//!   so neither is abandoned. A later claim or work, by anyone, begins a new stretch as usual.
//!
//! What nothing ended runs to the last event of all, open.

use std::collections::BTreeMap;

use swarmception_model::{Kind, StopReason, Timestamp};

use crate::activity::{Activity, IssueEvent, stop_reason};

/// What a ticket was doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Claimed, before work on it began.
    Claimed,
    Working,
    /// Its work finished (submitted), waiting for the merge.
    Waiting,
    /// The coordinator sent it back, before work began again.
    SentBack,
    /// Its worker stopped (handoff, blocked, paused) before finishing.
    Stopped,
}

impl Phase {
    /// How a person names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Working => "working",
            Self::Waiting => "waiting for merge",
            Self::SentBack => "sent back",
            Self::Stopped => "stopped",
        }
    }
}

/// A stretch of one phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stretch {
    pub phase: Phase,
    pub start: Timestamp,
    pub end: Timestamp,
    /// Nothing ended it: it runs to the last event.
    pub open: bool,
    /// The instance whose event began it.
    pub by: String,
    /// The note or reason the event that began it carried.
    pub note: Option<String>,
    /// Its holder's session ended, which ended it.
    pub abandoned: Option<Abandoned>,
    /// Working again after a send-back that its submit answered with no `work` logged between
    /// (#83): it starts [`SENT_BACK_MARK`] after the send-back, a time no event logged.
    pub rework: bool,
}

/// How long a send-back that a submit answered reads as *sent back*, in seconds, before the rest
/// of the time up to the submit reads as its rework (#83): long enough to be seen on the timeline
/// as a mark, as a merge's ten seconds are.
pub const SENT_BACK_MARK: i64 = 60;

/// The note a rework stretch carries, for its tooltip and the details.
pub const REWORK_NOTE: &str = "working again after the send-back (no work logged: from the send-back to the submit)";

/// A ticket left by its holder: the session of the instance that held it ended with a claimed,
/// working or sent back stretch going on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abandoned {
    /// When the session ended.
    pub t: Timestamp,
    /// The instance that held it, and the one that ended its session when another did (a
    /// coordinator's `session.stop --of`).
    pub holder: String,
    pub ended_by: Option<String>,
}

impl Abandoned {
    /// `abandoned: <instance>'s session ended <t>`, and who ended it when another did.
    #[must_use]
    pub fn text(&self) -> String {
        let by = self.ended_by.as_ref().map(|by| format!(" (ended by {by})")).unwrap_or_default();
        format!("abandoned: {}'s session ended {}{by}", self.holder, self.t)
    }
}

/// Who holds a ticket: the instance that last claimed, worked on or submitted it, a gate's events
/// included, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub instance: String,
    pub since: Timestamp,
}

/// A ticket's merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub t: Timestamp,
    pub pr: Option<u64>,
    /// The instance that merged it, and the gate it merged in.
    pub by: String,
    pub sub: Option<String>,
}

/// A ticket's `stop done` while it was claimed or worked on: it finished without a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub t: Timestamp,
    /// The instance that stopped it, and the note it gave.
    pub by: String,
    pub note: Option<String>,
}

/// One ticket's life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub issue: u64,
    /// The pull request its events name last.
    pub pr: Option<u64>,
    /// Its first claim, the first time work on it began, its last submit, and its merge.
    pub claimed: Option<Timestamp>,
    pub began: Option<Timestamp>,
    pub finished: Option<Timestamp>,
    pub merged: Option<Merged>,
    /// Its last `stop done` while claimed or worked on: it finished without a merge.
    pub done: Option<Done>,
    /// Its holder's session ended with it claimed, worked on or sent back, and nothing took it
    /// up after: neither open nor done.
    pub abandoned: Option<Abandoned>,
    /// Who holds it, whose session ending abandons it.
    pub holder: Option<Hold>,
    pub stretches: Vec<Stretch>,
    /// The instance that claimed it or worked on it last, not counting a gate's events.
    pub worker: Option<String>,
    /// Its first event's time: tickets are listed by it.
    pub first: Timestamp,
}

impl Ticket {
    fn new(issue: u64, first: Timestamp) -> Self {
        Self {
            issue,
            pr: None,
            claimed: None,
            began: None,
            finished: None,
            merged: None,
            done: None,
            abandoned: None,
            holder: None,
            stretches: Vec::new(),
            worker: None,
            first,
        }
    }

    /// `#<issue>`, or `#<issue> · PR #<pr>`.
    #[must_use]
    pub fn title(&self) -> String {
        self.pr.map_or_else(|| format!("#{}", self.issue), |pr| format!("#{} · PR #{pr}", self.issue))
    }

    /// The phase it is in now, if a stretch is going on.
    fn current(&self) -> Option<Phase> {
        self.stretches.last().filter(|s| s.open).map(|s| s.phase)
    }

    fn end(&mut self, t: Timestamp) {
        if let Some(last) = self.stretches.last_mut().filter(|s| s.open) {
            last.end = t.max(last.start);
            last.open = false;
        }
    }

    fn begin(&mut self, phase: Phase, event: &IssueEvent, note: Option<String>) {
        self.end(event.t);
        self.abandoned = None;
        self.stretches.push(Stretch { phase, start: event.t, end: event.t, open: true, by: event.instance.clone(), note, abandoned: None, rework: false });
    }

    /// A worker's `submit` straight after a send-back (#83): the send-back keeps its mark, and the
    /// rest of the time up to the submit is working again, by the submitter. A gate's submit, or
    /// one within the mark, changes nothing.
    fn rework(&mut self, event: &IssueEvent) {
        if event.sub.is_some() {
            return;
        }
        let Some(sent) = self.stretches.last_mut().filter(|s| s.open && s.phase == Phase::SentBack) else { return };
        let from = Timestamp(sent.start.0.saturating_add(SENT_BACK_MARK));
        if from >= event.t {
            return;
        }
        sent.end = from;
        sent.open = false;
        let by = event.instance.clone();
        self.stretches.push(Stretch {
            phase: Phase::Working,
            start: from,
            end: event.t,
            open: true,
            by,
            note: Some(REWORK_NOTE.to_owned()),
            abandoned: None,
            rework: true,
        });
    }

    /// Ends a claimed, working or sent back stretch where its holder's session ended, when that
    /// came before `before` (none: at any time): the ticket was abandoned.
    fn lapse(&mut self, before: Option<Timestamp>, activity: &Activity) {
        let Some(hold) = &self.holder else { return };
        let Some(open) = self.stretches.last_mut().filter(|s| s.open && matches!(s.phase, Phase::Claimed | Phase::Working | Phase::SentBack)) else {
            return;
        };
        let Some(session) = activity.agent(&hold.instance).and_then(|a| a.session_ended_after(hold.since)) else { return };
        if before.is_some_and(|before| session.span.end >= before) {
            return;
        }
        let abandoned = Abandoned { t: session.span.end.max(open.start), holder: hold.instance.clone(), ended_by: session.ended_by.clone() };
        open.end = abandoned.t;
        open.open = false;
        open.abandoned = Some(abandoned.clone());
        self.abandoned = Some(abandoned);
    }

    /// Who holds it, and its worker: a claim, work or submit's instance (a gate's not a worker).
    fn hold(&mut self, event: &IssueEvent) {
        if matches!(event.kind, Kind::Claim { .. } | Kind::Work { .. } | Kind::Submit { .. }) {
            self.holder = Some(Hold { instance: event.instance.clone(), since: event.t });
            if event.sub.is_none() {
                self.worker = Some(event.instance.clone());
            }
        }
    }

    /// Begins `phase` with `event`, unless the ticket is in one of `unless` already.
    fn enter(&mut self, phase: Phase, event: &IssueEvent, unless: &[Phase]) {
        if !self.current().is_some_and(|c| unless.contains(&c)) {
            self.begin(phase, event, event.note.clone());
        }
    }

    fn read(&mut self, event: &IssueEvent) {
        self.pr = event.kind.mr().or(self.pr);
        self.hold(event);
        match &event.kind {
            Kind::Claim { .. } => {
                self.claimed = self.claimed.or(Some(event.t));
                self.enter(Phase::Claimed, event, &[Phase::Claimed, Phase::Working, Phase::Waiting]);
            }
            Kind::Work { .. } => {
                self.began = self.began.or(Some(event.t));
                self.enter(Phase::Working, event, &[Phase::Working]);
            }
            Kind::Submit { .. } => {
                self.finished = Some(event.t);
                self.rework(event);
                self.enter(Phase::Waiting, event, &[Phase::Waiting]);
            }
            Kind::Sendback { .. } => self.begin(Phase::SentBack, event, event.note.clone()),
            Kind::Stop { reason, .. } => self.stop(event, reason.as_ref()),
            Kind::Release { .. } => self.end(event.t),
            Kind::Merge { mr, .. } => {
                self.end(event.t);
                self.abandoned = None;
                self.merged = Some(Merged { t: event.t, pr: mr.or(self.pr), by: event.instance.clone(), sub: event.sub.clone() });
            }
            _ => {}
        }
    }

    /// A stop, when the ticket was claimed or worked on: `done` ends the stretch and marks it done
    /// without a merge; any other reason starts *stopped*. Otherwise (waiting, say) nothing.
    fn stop(&mut self, event: &IssueEvent, reason: Option<&StopReason>) {
        if !matches!(self.current(), Some(Phase::Claimed | Phase::Working)) {
            return;
        }
        if matches!(reason, Some(StopReason::Done)) {
            self.end(event.t);
            self.done = Some(Done { t: event.t, by: event.instance.clone(), note: event.note.clone() });
        } else {
            let why = reason.map(stop_reason).or_else(|| event.note.clone());
            self.begin(Phase::Stopped, event, why);
        }
    }
}

/// Every ticket's life, in the order their first events came.
#[must_use]
pub fn tickets(activity: &Activity) -> Vec<Ticket> {
    let last = activity.extent.map_or(Timestamp::default(), |(_, end)| end);
    let mut tickets: BTreeMap<u64, Ticket> = BTreeMap::new();
    for event in &activity.issue_events {
        let ticket = tickets.entry(event.issue).or_insert_with(|| Ticket::new(event.issue, event.t));
        ticket.lapse(Some(event.t), activity);
        ticket.read(event);
    }
    let mut tickets: Vec<Ticket> = tickets.into_values().collect();
    for ticket in &mut tickets {
        ticket.lapse(None, activity);
        if let Some(open) = ticket.stretches.last_mut().filter(|s| s.open) {
            open.end = last.max(open.start);
        }
    }
    tickets.sort_by_key(|t| (t.first, t.issue));
    tickets
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::activity::tests::sample;

    #[test]
    fn a_ticket_from_claim_to_merge() {
        let activity = Activity::of(&sample());
        let tickets = tickets(&activity);
        assert_eq!(tickets.iter().map(|t| t.issue).collect::<Vec<_>>(), [168, 170], "by their first events");
        let ticket = &tickets[1];
        assert_eq!(ticket.title(), "#170 · PR #173");
        let at = |t: Option<Timestamp>| t.map(|t| t.to_string());
        assert_eq!(at(ticket.claimed).as_deref(), Some("2026-10-31T03:53:22Z"));
        assert_eq!(at(ticket.began).as_deref(), Some("2026-10-31T03:53:25Z"));
        assert_eq!(at(ticket.finished).as_deref(), Some("2026-10-31T04:06:43Z"));
        let merged = ticket.merged.as_ref().unwrap();
        assert_eq!((merged.t.to_string().as_str(), merged.by.as_str(), merged.sub.as_deref()), ("2026-10-31T04:28:58Z", "mini-8d44f3", Some("gate-pr173")));
        let phases: Vec<Phase> = ticket.stretches.iter().map(|s| s.phase).collect();
        assert_eq!(phases, [Phase::Claimed, Phase::Working, Phase::Waiting], "`stop done` after the submit changes nothing");
        assert!(ticket.stretches.iter().all(|s| !s.open));
        assert_eq!(ticket.stretches[2].end, merged.t);
        assert_eq!(ticket.worker.as_deref(), Some("mini-79ba63"), "the worker, not the gate that merged it");
    }

    #[test]
    fn work_whose_session_a_coordinator_ended_is_abandoned_there() {
        let activity = Activity::of(&sample());
        let ticket = &tickets(&activity)[0];
        assert_eq!(ticket.stretches.len(), 1);
        assert!(!ticket.stretches[0].open);
        assert_eq!(ticket.stretches[0].end.to_string(), "2026-10-31T05:00:00Z");
        let abandoned = ticket.abandoned.as_ref().unwrap();
        assert_eq!(abandoned.text(), "abandoned: mini-dead01's session ended 2026-10-31T05:00:00Z (ended by mini-8d44f3)");
        assert_eq!(ticket.stretches[0].abandoned.as_ref(), Some(abandoned));
        assert_eq!(ticket.claimed, None);
        assert!(ticket.merged.is_none() && ticket.done.is_none());
    }

    /// Tickets whose holders left, each in its own file: #40 worked on till its worker's
    /// `session.stop` at 01:10, then taken up and merged by worker-3; #41 worked on by a session
    /// that never logged a stop; #42 and #43 (working, then stopped blocked) held by a session the
    /// coordinator ends at 04:00; #44 sent back after its worker had left; #45 claimed by a
    /// session its instance's next `session.start` ends; #46 submitted, then its worker left.
    pub(crate) const ABANDONED: [(&str, &str); 7] = [
        (
            "m-1",
            r#"{"event":"session.start","machine":"m","name":"worker-1","role":"worker","t":"2026-10-31T01:00:00Z"}
{"event":"claim","issue":40,"t":"2026-10-31T01:00:10Z"}
{"event":"work","issue":40,"t":"2026-10-31T01:00:15Z"}
{"event":"session.stop","note":"review done","t":"2026-10-31T01:10:00Z"}"#,
        ),
        (
            "m-2",
            r#"{"event":"session.start","machine":"m","name":"worker-2","role":"worker","t":"2026-10-31T01:05:00Z"}
{"event":"claim","issue":41,"t":"2026-10-31T01:05:10Z"}
{"event":"work","issue":41,"t":"2026-10-31T01:05:20Z"}"#,
        ),
        (
            "m-3",
            r#"{"event":"session.start","machine":"m","name":"worker-3","role":"worker","t":"2026-10-31T02:00:00Z"}
{"event":"claim","issue":40,"mr":50,"t":"2026-10-31T02:00:10Z"}
{"event":"work","issue":40,"t":"2026-10-31T02:00:20Z"}
{"event":"submit","issue":40,"mr":50,"t":"2026-10-31T02:30:00Z"}
{"event":"session.stop","t":"2026-10-31T02:40:00Z"}"#,
        ),
        (
            "m-4",
            r#"{"event":"session.start","machine":"m","name":"worker-4","role":"worker","t":"2026-10-31T01:20:00Z"}
{"event":"claim","issue":42,"t":"2026-10-31T01:20:10Z"}
{"event":"work","issue":42,"t":"2026-10-31T01:20:20Z"}
{"event":"claim","issue":43,"t":"2026-10-31T01:25:00Z"}
{"event":"stop","issue":43,"reason":"blocked","t":"2026-10-31T01:26:00Z"}"#,
        ),
        (
            "m-5",
            r#"{"event":"session.start","machine":"m","name":"worker-5","role":"worker","t":"2026-10-31T01:30:00Z"}
{"event":"claim","issue":44,"mr":51,"t":"2026-10-31T01:30:10Z"}
{"event":"work","issue":44,"t":"2026-10-31T01:30:20Z"}
{"event":"submit","issue":44,"mr":51,"t":"2026-10-31T01:40:00Z"}
{"event":"claim","issue":46,"mr":52,"t":"2026-10-31T01:41:00Z"}
{"event":"submit","issue":46,"mr":52,"t":"2026-10-31T01:42:00Z"}
{"event":"session.stop","t":"2026-10-31T01:45:00Z"}"#,
        ),
        (
            "m-6",
            r#"{"event":"session.start","machine":"m","name":"worker-6","role":"worker","t":"2026-10-31T01:00:00Z"}
{"event":"claim","issue":45,"t":"2026-10-31T01:01:00Z"}
{"event":"session.start","machine":"m","name":"worker-6","role":"worker","t":"2026-10-31T01:30:00Z"}"#,
        ),
        (
            "m-c",
            r#"{"event":"session.start","machine":"m","name":"coordinator","role":"coordinator","t":"2026-10-31T00:59:00Z"}
{"event":"sendback","issue":44,"mr":51,"note":"tests fail","t":"2026-10-31T02:00:00Z"}
{"event":"merge","issue":40,"mr":50,"t":"2026-10-31T03:00:00Z"}
{"event":"session.stop","of":"m-4","note":"silent 3 h","t":"2026-10-31T04:00:00Z"}"#,
        ),
    ];

    pub(crate) fn abandoned() -> Vec<swarmception_model::AgentFile> {
        ABANDONED.iter().map(|(instance, text)| swarmception_model::parse_file(instance, text)).collect()
    }

    fn ticket(tickets: &[Ticket], issue: u64) -> &Ticket {
        tickets.iter().find(|t| t.issue == issue).unwrap()
    }

    fn spans(ticket: &Ticket) -> Vec<(Phase, String, String, bool, Option<String>)> {
        ticket.stretches.iter().map(|s| (s.phase, s.start.to_string(), s.end.to_string(), s.open, s.abandoned.as_ref().map(|a| a.holder.clone()))).collect()
    }

    fn at(text: &str) -> String {
        format!("2026-10-31T{text}Z")
    }

    #[test]
    fn work_ends_where_its_workers_session_did_and_another_takes_it_up() {
        let tickets = tickets(&Activity::of(&abandoned()));
        let ticket = ticket(&tickets, 40);
        assert_eq!(
            spans(ticket),
            [
                (Phase::Claimed, at("01:00:10"), at("01:00:15"), false, None),
                (Phase::Working, at("01:00:15"), at("01:10:00"), false, Some("m-1".to_owned())),
                (Phase::Claimed, at("02:00:10"), at("02:00:20"), false, None),
                (Phase::Working, at("02:00:20"), at("02:30:00"), false, None),
                (Phase::Waiting, at("02:30:00"), at("03:00:00"), false, None),
            ],
            "abandoned at worker-1's stop; worker-3's claim begins anew, and its stop after the submit leaves the wait alone"
        );
        assert_eq!(ticket.stretches[1].abandoned.as_ref().map(Abandoned::text).as_deref(), Some("abandoned: m-1's session ended 2026-10-31T01:10:00Z"));
        assert_eq!(ticket.abandoned, None, "taken up and merged: not abandoned");
        assert!(ticket.merged.is_some());
        assert_eq!(ticket.worker.as_deref(), Some("m-3"));
    }

    #[test]
    fn a_holder_whose_session_goes_on_keeps_it_open() {
        let tickets = tickets(&Activity::of(&abandoned()));
        let ticket = ticket(&tickets, 41);
        assert_eq!(spans(ticket)[1], (Phase::Working, at("01:05:20"), at("04:00:00"), true, None), "no stop logged: still going, to the last event");
        assert_eq!(ticket.abandoned, None);
    }

    #[test]
    fn a_coordinators_stop_of_a_dead_session_abandons_what_it_held_but_not_what_it_stopped() {
        let tickets = tickets(&Activity::of(&abandoned()));
        let working = ticket(&tickets, 42);
        assert_eq!(spans(working)[1], (Phase::Working, at("01:20:20"), at("04:00:00"), false, Some("m-4".to_owned())));
        assert_eq!(working.abandoned.as_ref().and_then(|a| a.ended_by.as_deref()), Some("m-c"));
        let stopped = ticket(&tickets, 43);
        assert_eq!(spans(stopped)[1], (Phase::Stopped, at("01:26:00"), at("04:00:00"), true, None), "set down on purpose: stays stopped");
        assert_eq!(stopped.abandoned, None);
    }

    #[test]
    fn sent_back_after_its_worker_left_is_abandoned_at_the_send_back() {
        let tickets = tickets(&Activity::of(&abandoned()));
        let ticket = ticket(&tickets, 44);
        assert_eq!(
            spans(ticket)[2..],
            [(Phase::Waiting, at("01:40:00"), at("02:00:00"), false, None), (Phase::SentBack, at("02:00:00"), at("02:00:00"), false, Some("m-5".to_owned())),]
        );
        assert_eq!(ticket.abandoned.as_ref().map(|a| a.t.to_string()), Some(at("02:00:00")));
    }

    #[test]
    fn a_new_session_start_ends_the_last_and_a_wait_is_never_abandoned() {
        let tickets = tickets(&Activity::of(&abandoned()));
        assert_eq!(spans(ticket(&tickets, 45)), [(Phase::Claimed, at("01:01:00"), at("01:30:00"), false, Some("m-6".to_owned()))]);
        let waiting = ticket(&tickets, 46);
        assert_eq!(spans(waiting)[1], (Phase::Waiting, at("01:42:00"), at("04:00:00"), true, None), "submitted: it waits for the coordinator");
        assert_eq!(waiting.abandoned, None);
    }

    #[test]
    fn a_sendback_and_a_release() {
        let worker = r#"{"event":"claim","issue":5,"t":"2026-10-31T01:00:00Z"}
{"event":"work","issue":5,"t":"2026-10-31T01:00:05Z"}
{"event":"submit","issue":5,"mr":9,"t":"2026-10-31T01:10:00Z"}
{"event":"work","issue":5,"t":"2026-10-31T01:30:00Z"}
{"event":"submit","issue":5,"mr":9,"t":"2026-10-31T01:40:00Z"}
{"event":"claim","issue":6,"t":"2026-10-31T01:41:00Z"}
{"event":"stop","issue":6,"reason":"blocked","t":"2026-10-31T01:45:00Z"}
{"event":"release","issue":6,"t":"2026-10-31T01:50:00Z"}"#;
        let coordinator = r#"{"event":"sendback","issue":5,"mr":9,"note":"tests fail","t":"2026-10-31T01:20:00Z"}
{"event":"merge","issue":5,"mr":9,"t":"2026-10-31T02:00:00Z"}"#;
        let files = [swarmception_model::parse_file("m-1", worker), swarmception_model::parse_file("m-2", coordinator)];
        let tickets = tickets(&Activity::of(&files));
        let phases = |t: &Ticket| t.stretches.iter().map(|s| s.phase).collect::<Vec<_>>();
        assert_eq!(phases(&tickets[0]), [Phase::Claimed, Phase::Working, Phase::Waiting, Phase::SentBack, Phase::Working, Phase::Waiting]);
        assert_eq!(tickets[0].stretches[3].note.as_deref(), Some("tests fail"));
        assert_eq!(tickets[0].finished.map(|t| t.to_string()).as_deref(), Some("2026-10-31T01:40:00Z"), "the last submit");
        assert_eq!(phases(&tickets[1]), [Phase::Claimed, Phase::Stopped]);
        assert_eq!(tickets[1].stretches[1].note.as_deref(), Some("blocked"));
        assert!(!tickets[1].stretches[1].open, "the release ended it");
    }

    /// mc-engine #169 as its tracking branch has it (#83, the owner's): alder claims, works and
    /// submits; the coordinator sends it back for a conflict; alder fixes it and submits again,
    /// with no `work` logged between; the gate merges it.
    pub(crate) const SENT_BACK_169: [(&str, &str); 2] = [
        (
            "christohersmini-314f61",
            r#"{"effort":"high","event":"session.start","format":3,"machine":"christohersmini","model":"claude-opus-5-5","name":"alder","parent":"christohersmini-dadb36","role":"worker","t":"2026-11-01T10:56:01Z"}
{"event":"claim","issue":169,"mr":178,"t":"2026-11-01T10:56:32Z"}
{"event":"work","issue":169,"t":"2026-11-01T10:56:34Z"}
{"event":"sub.start","name":"ready","note":"ci --changed before submitting","sub":"ready-a4d8225","t":"2026-11-01T11:21:25Z"}
{"event":"sub.stop","note":"rc=0","sub":"ready-a4d8225","t":"2026-11-01T11:22:53Z"}
{"event":"submit","issue":169,"mr":178,"t":"2026-11-01T11:23:00Z"}
{"event":"sub.start","name":"ready","note":"ci --changed before submitting","sub":"ready-aec5c5f","t":"2026-11-01T11:34:59Z"}
{"event":"sub.stop","note":"rc=0","sub":"ready-aec5c5f","t":"2026-11-01T11:35:54Z"}
{"event":"submit","issue":169,"mr":178,"t":"2026-11-01T11:36:00Z"}"#,
        ),
        (
            "christohersmini-dadb36",
            r##"{"event":"session.start","format":3,"machine":"christohersmini","model":"claude-opus-5-5","name":"coordinator","note":"setup: workspace, spec, first issues, workers","role":"coordinator","t":"2026-10-31T10:59:14Z"}
{"event":"sendback","issue":169,"mr":178,"note":"cause=conflict: crates/core/src/constants.rs, crates/core/src/scene/node.rs, crates/engine/src/world.rs (with #177, the hierarchy)","t":"2026-11-01T11:32:29Z"}
{"event":"sub.start","name":"gate","note":"#178 colliders, conflicts with audio resolved; capacity=3 free=17.9; kind=smoke (22 files changed, not only Markdown)","sub":"gate-pr178","t":"2026-11-01T12:07:03Z"}
{"event":"merge","issue":169,"mr":178,"sub":"gate-pr178","t":"2026-11-01T12:07:37Z"}
{"event":"sub.stop","note":"rc=0; kind=smoke; merged #178","sub":"gate-pr178","t":"2026-11-01T12:07:43Z"}"##,
        ),
    ];

    /// #7 sent back after its worker left, then re-claimed by another worker on a new branch
    /// (#83's fixture): claimed and worked on by worker-1, sent back at 01:30 when worker-1's
    /// session had ended; worker-2 claims it at 02:00 with a new PR, works and submits; merged.
    pub(crate) const RECLAIMED: [(&str, &str); 3] = [
        (
            "m-1",
            r#"{"event":"session.start","machine":"m","name":"worker-1","role":"worker","t":"2026-11-01T01:00:00Z"}
{"event":"claim","issue":7,"mr":20,"t":"2026-11-01T01:00:10Z"}
{"event":"work","issue":7,"t":"2026-11-01T01:00:20Z"}
{"event":"submit","issue":7,"mr":20,"t":"2026-11-01T01:20:00Z"}
{"event":"session.stop","t":"2026-11-01T01:25:00Z"}"#,
        ),
        (
            "m-2",
            r#"{"event":"session.start","machine":"m","name":"worker-2","role":"worker","t":"2026-11-01T01:50:00Z"}
{"event":"claim","issue":7,"mr":21,"t":"2026-11-01T02:00:00Z"}
{"event":"work","issue":7,"t":"2026-11-01T02:00:10Z"}
{"event":"submit","issue":7,"mr":21,"t":"2026-11-01T02:30:00Z"}"#,
        ),
        (
            "m-c",
            r#"{"event":"session.start","machine":"m","name":"coordinator","role":"coordinator","t":"2026-11-01T00:59:00Z"}
{"event":"sendback","issue":7,"mr":20,"note":"cause=test","t":"2026-11-01T01:30:00Z"}
{"event":"merge","issue":7,"mr":21,"t":"2026-11-01T03:00:00Z"}"#,
        ),
    ];

    pub(crate) fn files(files: &[(&str, &str)]) -> Vec<swarmception_model::AgentFile> {
        files.iter().map(|(instance, text)| swarmception_model::parse_file(instance, text)).collect()
    }

    #[test]
    fn a_submit_straight_after_a_send_back_is_its_rework() {
        let tickets = tickets(&Activity::of(&files(&SENT_BACK_169)));
        let ticket = ticket(&tickets, 169);
        let day = |t: &str| format!("2026-11-01T{t}Z");
        let spans: Vec<_> = ticket.stretches.iter().map(|s| (s.phase, s.start.to_string(), s.end.to_string(), s.rework)).collect();
        assert_eq!(
            spans,
            [
                (Phase::Claimed, day("10:56:32"), day("10:56:34"), false),
                (Phase::Working, day("10:56:34"), day("11:23:00"), false),
                (Phase::Waiting, day("11:23:00"), day("11:32:29"), false),
                (Phase::SentBack, day("11:32:29"), day("11:33:29"), false),
                (Phase::Working, day("11:33:29"), day("11:36:00"), true),
                (Phase::Waiting, day("11:36:00"), day("12:07:37"), false),
            ],
            "the send-back a minute's mark, alder's fix working again up to the second submit"
        );
        let rework = &ticket.stretches[4];
        assert_eq!((rework.by.as_str(), rework.note.as_deref(), rework.open), ("christohersmini-314f61", Some(REWORK_NOTE), false));
        assert_eq!(ticket.stretches[3].note.as_deref().map(|n| n.starts_with("cause=conflict")), Some(true), "the send-back keeps its cause");
        assert_eq!(ticket.began.map(|t| t.to_string()), Some(day("10:56:34")), "work first began at the work event");
        assert_eq!(ticket.finished.map(|t| t.to_string()), Some(day("11:36:00")));
        assert_eq!(ticket.worker.as_deref(), Some("christohersmini-314f61"));
    }

    #[test]
    fn a_send_back_stays_one_until_a_workers_submit_answers_it_and_a_quick_submit_keeps_it_whole() {
        let worker = r#"{"event":"session.start","machine":"m","name":"worker-1","role":"worker","t":"2026-11-01T00:59:00Z"}
{"event":"claim","issue":1,"t":"2026-11-01T01:00:00Z"}
{"event":"work","issue":1,"t":"2026-11-01T01:00:05Z"}
{"event":"submit","issue":1,"mr":9,"t":"2026-11-01T01:10:00Z"}
{"event":"submit","issue":1,"mr":9,"t":"2026-11-01T01:20:30Z"}
{"event":"claim","issue":2,"t":"2026-11-01T01:30:00Z"}
{"event":"work","issue":2,"t":"2026-11-01T01:30:05Z"}
{"event":"submit","issue":2,"mr":10,"t":"2026-11-01T01:40:00Z"}
{"event":"claim","issue":3,"t":"2026-11-01T02:00:00Z"}
{"event":"work","issue":3,"t":"2026-11-01T02:00:05Z"}
{"event":"submit","issue":3,"mr":11,"t":"2026-11-01T02:10:00Z"}"#;
        let coordinator = r#"{"event":"session.start","machine":"m","name":"coordinator","role":"coordinator","t":"2026-11-01T00:59:00Z"}
{"event":"sendback","issue":1,"mr":9,"note":"cause=test","t":"2026-11-01T01:20:00Z"}
{"event":"sendback","issue":2,"mr":10,"note":"cause=build","t":"2026-11-01T01:50:00Z"}
{"event":"sub.start","name":"gate","sub":"gate-pr10","t":"2026-11-01T01:55:00Z"}
{"event":"submit","issue":2,"mr":10,"sub":"gate-pr10","t":"2026-11-01T01:58:00Z"}
{"event":"sub.stop","note":"rc=0","sub":"gate-pr10","t":"2026-11-01T01:59:00Z"}
{"event":"sendback","issue":3,"mr":11,"note":"cause=clippy","t":"2026-11-01T02:20:00Z"}
{"event":"sub.start","name":"gate","sub":"gate-late","t":"2026-11-01T02:40:00Z"}"#;
        let tickets = tickets(&Activity::of(&files(&[("m-1", worker), ("m-c", coordinator)])));
        let phases = |t: &Ticket| t.stretches.iter().map(|s| (s.phase, s.rework)).collect::<Vec<_>>();
        assert_eq!(
            phases(ticket(&tickets, 1)),
            [(Phase::Claimed, false), (Phase::Working, false), (Phase::Waiting, false), (Phase::SentBack, false), (Phase::Waiting, false)],
            "submitted again within the mark: the send-back runs to it"
        );
        let gated = ticket(&tickets, 2);
        assert_eq!(gated.stretches[3].phase, Phase::SentBack);
        assert!(gated.stretches.iter().all(|s| !s.rework), "a gate's submit is no worker's rework");
        let pending = ticket(&tickets, 3);
        assert_eq!(
            pending.stretches.last().map(|s| (s.phase, s.open, s.rework)),
            Some((Phase::SentBack, true, false)),
            "nothing answered it yet: still sent back"
        );
    }

    #[test]
    fn a_ticket_re_claimed_by_another_worker_after_a_send_back_begins_anew() {
        let tickets = tickets(&Activity::of(&files(&RECLAIMED)));
        let ticket = ticket(&tickets, 7);
        let spans: Vec<_> = ticket.stretches.iter().map(|s| (s.phase, s.by.as_str(), s.rework)).collect();
        assert_eq!(
            spans,
            [
                (Phase::Claimed, "m-1", false),
                (Phase::Working, "m-1", false),
                (Phase::Waiting, "m-1", false),
                (Phase::SentBack, "m-c", false),
                (Phase::Claimed, "m-2", false),
                (Phase::Working, "m-2", false),
                (Phase::Waiting, "m-2", false),
            ]
        );
        assert_eq!(ticket.stretches[3].abandoned.as_ref().map(|a| a.holder.as_str()), Some("m-1"), "worker-1 had left: abandoned at the send-back");
        assert_eq!((ticket.worker.as_deref(), ticket.pr), (Some("m-2"), Some(21)));
    }

    /// The events of mc-engine #58 (an architecture review whose output was new issues), then a
    /// ticket done while only claimed, then a later event the old reading ran both on to.
    pub(crate) const DONE: &str = r#"{"event":"claim","issue":58,"mr":64,"t":"2026-10-31T12:59:16Z"}
{"event":"work","issue":58,"t":"2026-10-31T12:59:18Z"}
{"event":"stop","issue":58,"reason":"done","note":"review filed as #66-#73","t":"2026-10-31T13:06:43Z"}
{"event":"claim","issue":59,"t":"2026-10-31T13:10:00Z"}
{"event":"stop","issue":59,"reason":"done","t":"2026-10-31T13:12:00Z"}
{"event":"work","issue":60,"t":"2026-10-31T22:00:00Z"}"#;

    #[test]
    fn stop_done_while_worked_on_ends_the_stretch() {
        let tickets = tickets(&Activity::of(&[swarmception_model::parse_file("m-1", DONE)]));
        let ticket = &tickets[0];
        assert_eq!(ticket.stretches.iter().map(|s| s.phase).collect::<Vec<_>>(), [Phase::Claimed, Phase::Working]);
        let working = &ticket.stretches[1];
        assert!(!working.open, "the stop ended it");
        assert_eq!((working.start.to_string(), working.end.to_string()), ("2026-10-31T12:59:18Z".to_owned(), "2026-10-31T13:06:43Z".to_owned()));
        let done = ticket.done.as_ref().unwrap();
        assert_eq!((done.t.to_string().as_str(), done.by.as_str(), done.note.as_deref()), ("2026-10-31T13:06:43Z", "m-1", Some("review filed as #66-#73")));
        assert!(ticket.merged.is_none() && ticket.finished.is_none());
        let claimed = &tickets[1];
        assert_eq!(claimed.stretches.len(), 1);
        assert!(!claimed.stretches[0].open, "done while only claimed ends it too");
        assert_eq!(claimed.stretches[0].end.to_string(), "2026-10-31T13:12:00Z");
        assert!(claimed.done.is_some());
    }

    #[test]
    fn stop_done_after_a_submit_changes_nothing() {
        let activity = Activity::of(&sample());
        let ticket = &tickets(&activity)[1];
        assert!(ticket.done.is_none(), "the sample's `stop done` comes after #170's submit");
        assert_eq!(ticket.stretches.last().map(|s| s.phase), Some(Phase::Waiting));
    }
}
