//! Each ticket's life, from its issue events (Swarm Core's docs/tracking-format.md): when it was
//! claimed, when work on it began, when the work finished (its `submit`), when it merged, and the
//! stretches between. A ticket's events are read in time order across every agent's file:
//!
//! - `claim` starts a *claimed* stretch, unless the ticket is already claimed, worked on or waiting;
//! - `work` starts *working*; `submit` starts *waiting* for the merge;
//! - `sendback` starts *sent back*, until work begins again;
//! - `stop` but `done` starts *stopped* (a handoff, a block, a pause); `stop done` changes nothing;
//! - `release` ends the stretch, and `merge` ends it and marks the ticket merged.
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
    pub stretches: Vec<Stretch>,
    /// The instance that claimed it or worked on it last, not counting a gate's events.
    pub worker: Option<String>,
    /// Its first event's time: tickets are listed by it.
    pub first: Timestamp,
}

impl Ticket {
    fn new(issue: u64, first: Timestamp) -> Self {
        Self { issue, pr: None, claimed: None, began: None, finished: None, merged: None, stretches: Vec::new(), worker: None, first }
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
        self.stretches.push(Stretch { phase, start: event.t, end: event.t, open: true, by: event.instance.clone(), note });
    }

    /// Begins `phase` with `event`, unless the ticket is in one of `unless` already.
    fn enter(&mut self, phase: Phase, event: &IssueEvent, unless: &[Phase]) {
        if !self.current().is_some_and(|c| unless.contains(&c)) {
            self.begin(phase, event, event.note.clone());
        }
    }

    fn read(&mut self, event: &IssueEvent) {
        self.pr = event.kind.mr().or(self.pr);
        if event.sub.is_none() && matches!(event.kind, Kind::Claim { .. } | Kind::Work { .. } | Kind::Submit { .. }) {
            self.worker = Some(event.instance.clone());
        }
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
                self.enter(Phase::Waiting, event, &[Phase::Waiting]);
            }
            Kind::Sendback { .. } => self.begin(Phase::SentBack, event, event.note.clone()),
            Kind::Stop { reason, .. } if !matches!(reason, Some(StopReason::Done)) => self.stop(event, reason.as_ref()),
            Kind::Release { .. } => self.end(event.t),
            Kind::Merge { mr, .. } => {
                self.end(event.t);
                self.merged = Some(Merged { t: event.t, pr: mr.or(self.pr), by: event.instance.clone(), sub: event.sub.clone() });
            }
            _ => {}
        }
    }

    /// A stop but `done`: the ticket stopped, when it was claimed or worked on.
    fn stop(&mut self, event: &IssueEvent, reason: Option<&StopReason>) {
        if matches!(self.current(), Some(Phase::Claimed | Phase::Working)) {
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
        tickets.entry(event.issue).or_insert_with(|| Ticket::new(event.issue, event.t)).read(event);
    }
    let mut tickets: Vec<Ticket> = tickets.into_values().collect();
    for ticket in &mut tickets {
        if let Some(open) = ticket.stretches.last_mut().filter(|s| s.open) {
            open.end = last.max(open.start);
        }
    }
    tickets.sort_by_key(|t| (t.first, t.issue));
    tickets
}

#[cfg(test)]
mod tests {
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
    fn work_nothing_ended_runs_on() {
        let activity = Activity::of(&sample());
        let ticket = &tickets(&activity)[0];
        assert_eq!(ticket.stretches.len(), 1);
        assert!(ticket.stretches[0].open);
        assert_eq!(ticket.stretches[0].end.to_string(), "2026-10-31T05:00:00Z");
        assert_eq!(ticket.claimed, None);
        assert!(ticket.merged.is_none());
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
}
