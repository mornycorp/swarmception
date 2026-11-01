//! Each agent instance's activity, read from its events (Swarm Core's docs/tracking-format.md):
//! its sessions (split at its pauses), the spans it worked on each issue, its helper sub-agents
//! and the instants worth a mark. What another instance's events end (a coordinator's
//! `session.stop` or `release` with `of`) is applied in a second pass, across files.

use std::collections::BTreeMap;

use swarmception_model::{AgentFile, Event, Kind, Role, Timestamp};

/// A span of time, `[start, end)`; `open` when nothing ended it, so it runs to the last event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: Timestamp,
    pub end: Timestamp,
    pub open: bool,
}

/// A session: from its `session.start` to its `session.stop`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub span: Span,
    /// The spans it was paused, inside it.
    pub pauses: Vec<Span>,
    /// The note its start or stop carried.
    pub notes: Vec<String>,
    /// Who ended it, when another instance did (a coordinator ending a dead agent's session).
    pub ended_by: Option<String>,
}

/// A span of work on an issue: from its `claim` or `work` to its `stop`, `release`, a `work` on
/// another issue or the session's end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub issue: u64,
    pub mr: Option<u64>,
    pub span: Span,
    /// What ended it: `stop (done)`, `release`, `session end`, …
    pub ended: Option<String>,
}

/// A helper sub-agent: from its `sub.start` to its `sub.stop`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sub {
    pub sub: String,
    pub name: Option<String>,
    pub model: Option<String>,
    pub span: Span,
    pub start_note: Option<String>,
    pub stop_note: Option<String>,
    /// The exit code its stop note names first (`rc=<n>`): a gate's or a `ready`'s result.
    pub rc: Option<i64>,
    /// The issues its events name, and the pull requests its start note names (`#<m>`).
    pub issues: Vec<u64>,
    pub prs: Vec<u64>,
}

impl Sub {
    /// A coordinator's gate (`sub` beginning `gate`).
    #[must_use]
    pub fn is_gate(&self) -> bool {
        self.sub.starts_with("gate") || self.name.as_deref() == Some("gate")
    }
}

/// An instant worth a mark: a claim, a submit, a merge, a send-back, a release, a stop, an
/// event this reader doesn't know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub t: Timestamp,
    /// The event's name as the format writes it.
    pub event: String,
    pub issue: Option<u64>,
    pub mr: Option<u64>,
    pub note: Option<String>,
    pub sub: Option<String>,
    /// `reason`, for a stop or a release.
    pub reason: Option<String>,
}

/// One agent instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub instance: String,
    pub machine: String,
    /// Its session's name (`worker-2`, `coordinator`): the worker it is.
    pub name: String,
    pub role: Role,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub parent: Option<String>,
    pub sessions: Vec<Session>,
    pub work: Vec<Work>,
    pub subs: Vec<Sub>,
    pub marks: Vec<Mark>,
    /// Its events, all of them.
    pub events: usize,
}

/// A line that didn't read, by file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError {
    pub instance: String,
    pub line: usize,
    pub message: String,
}

/// An event that names an issue, with the instance that logged it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueEvent {
    pub t: Timestamp,
    pub instance: String,
    pub issue: u64,
    pub kind: Kind,
    pub sub: Option<String>,
    pub note: Option<String>,
}

/// Every agent instance's activity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Activity {
    /// By machine, then name, then instance.
    pub agents: Vec<Agent>,
    /// Every issue event of every file, oldest first (a file's own order kept at a tie).
    pub issue_events: Vec<IssueEvent>,
    /// The first and last event's times; none with no events.
    pub extent: Option<(Timestamp, Timestamp)>,
    /// Every event's time, of every file, oldest first: where the quiet gaps are.
    pub times: Vec<Timestamp>,
    pub errors: Vec<FileError>,
}

impl Activity {
    /// The activity of `files`.
    #[must_use]
    pub fn of(files: &[AgentFile]) -> Self {
        let times = files.iter().flat_map(|f| f.events.iter().map(|e| e.t));
        let extent = times.clone().min().zip(times.max());
        let last = extent.map_or(Timestamp::default(), |(_, end)| end);
        let mut agents: Vec<Agent> = files.iter().map(|file| agent(file, last)).collect();
        apply_others(&mut agents, files);
        agents.sort_by(|a, b| (&a.machine, &a.name, first_start(a), &a.instance).cmp(&(&b.machine, &b.name, first_start(b), &b.instance)));
        let errors =
            files.iter().flat_map(|f| f.errors.iter().map(|e| FileError { instance: f.instance.clone(), line: e.line, message: e.message.clone() })).collect();
        let mut issue_events: Vec<IssueEvent> = files
            .iter()
            .flat_map(|f| {
                f.events.iter().filter_map(|e| {
                    let issue = e.kind.issue()?;
                    Some(IssueEvent { t: e.t, instance: f.instance.clone(), issue, kind: e.kind.clone(), sub: e.sub.clone(), note: e.note.clone() })
                })
            })
            .collect();
        issue_events.sort_by_key(|e| e.t);
        let mut times: Vec<Timestamp> = files.iter().flat_map(|f| f.events.iter().map(|e| e.t)).collect();
        times.sort_unstable();
        Self { agents, issue_events, extent, times, errors }
    }

    /// The agent instance `instance`.
    #[must_use]
    pub fn agent(&self, instance: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.instance == instance)
    }

    /// How many events there are.
    #[must_use]
    pub fn events(&self) -> usize {
        self.agents.iter().map(|a| a.events).sum()
    }

    /// The pull request each issue's events name, by issue.
    #[must_use]
    pub fn prs(&self) -> BTreeMap<u64, u64> {
        let mut prs = BTreeMap::new();
        for mark in self.agents.iter().flat_map(|a| &a.marks) {
            if let (Some(issue), Some(mr)) = (mark.issue, mark.mr) {
                prs.insert(issue, mr);
            }
        }
        prs
    }
}

fn first_start(agent: &Agent) -> Option<Timestamp> {
    agent.sessions.first().map(|s| s.span.start)
}

/// The machine an instance id names: all before its last `-`.
fn machine_of(instance: &str) -> String {
    instance.rsplit_once('-').map_or(instance, |(machine, _)| machine).to_owned()
}

/// What one file's events say, before any other file's are applied.
#[derive(Default)]
struct Reading {
    session: Option<Session>,
    work: Option<Work>,
    pause: Option<Timestamp>,
    subs: BTreeMap<String, Sub>,
}

fn agent(file: &AgentFile, last: Timestamp) -> Agent {
    let mut agent = Agent {
        instance: file.instance.clone(),
        machine: machine_of(&file.instance),
        name: file.instance.clone(),
        role: Role::Other(String::new()),
        model: None,
        effort: None,
        parent: None,
        sessions: Vec::new(),
        work: Vec::new(),
        subs: Vec::new(),
        marks: Vec::new(),
        events: file.events.len(),
    };
    let mut reading = Reading::default();
    for event in &file.events {
        read(&mut agent, &mut reading, event);
    }
    // What nothing ended runs to the last event of all.
    end_session(&mut agent, &mut reading, last, None, true);
    let mut subs: Vec<Sub> = reading
        .subs
        .into_values()
        .map(|mut s| {
            s.span.end = last.max(s.span.start);
            s.span.open = true;
            s
        })
        .collect();
    agent.subs.append(&mut subs);
    agent.subs.sort_by_key(|s| s.span.start);
    agent
}

fn read(agent: &mut Agent, reading: &mut Reading, event: &Event) {
    let t = event.t;
    if reading.session.is_none() && !matches!(event.kind, Kind::SessionStart { .. } | Kind::SessionStop { .. }) {
        // An event before any start: a session the file never began.
        reading.session = Some(Session { span: Span { start: t, end: t, open: false }, pauses: Vec::new(), notes: Vec::new(), ended_by: None });
    }
    if let Some(sub) = &event.sub
        && let Some(issue) = event.kind.issue()
    {
        // An issue event a sub-agent did: the sub's, not the session's work.
        if let Some(open) = reading.subs.get_mut(sub) {
            push_unique(&mut open.issues, issue);
        } else if let Some(done) = agent.subs.iter_mut().rev().find(|s| &s.sub == sub) {
            push_unique(&mut done.issues, issue);
        }
        mark(agent, event);
        return;
    }
    match &event.kind {
        Kind::SessionStart { .. } | Kind::SessionStop { .. } | Kind::SessionPause | Kind::SessionResume => read_session(agent, reading, event),
        Kind::Claim { .. } | Kind::Work { .. } | Kind::Stop { .. } | Kind::Release { of: None, .. } | Kind::Submit { mr: Some(_), .. } => {
            read_work(agent, reading, event)
        }
        Kind::SubStart { .. } | Kind::SubStop { .. } => read_sub(agent, reading, event),
        _ => mark(agent, event),
    }
}

fn read_session(agent: &mut Agent, reading: &mut Reading, event: &Event) {
    let t = event.t;
    match &event.kind {
        Kind::SessionStart { name, role, machine, parent, model, effort, .. } => {
            end_session(agent, reading, t, None, false);
            agent.name = if name.is_empty() { agent.instance.clone() } else { name.clone() };
            agent.role = role.clone();
            if !machine.is_empty() {
                agent.machine = machine.clone();
            }
            agent.parent = parent.clone().or_else(|| agent.parent.take());
            agent.model = model.clone().or_else(|| agent.model.take());
            agent.effort = effort.clone().or_else(|| agent.effort.take());
            let notes = event.note.iter().cloned().collect();
            reading.session = Some(Session { span: Span { start: t, end: t, open: false }, pauses: Vec::new(), notes, ended_by: None });
        }
        Kind::SessionStop { of: Some(_), .. } => mark(agent, event),
        Kind::SessionStop { of: None, .. } => {
            if let (Some(session), Some(note)) = (reading.session.as_mut(), &event.note) {
                session.notes.push(note.clone());
            }
            end_session(agent, reading, t, None, false);
        }
        Kind::SessionPause => reading.pause = reading.pause.or(Some(t)),
        Kind::SessionResume => {
            if let (Some(since), Some(session)) = (reading.pause.take(), reading.session.as_mut()) {
                session.pauses.push(Span { start: since, end: t, open: false });
            }
        }
        _ => {}
    }
}

fn read_work(agent: &mut Agent, reading: &mut Reading, event: &Event) {
    let t = event.t;
    let working_on = |reading: &Reading, issue: u64| reading.work.as_ref().is_some_and(|w| w.issue == issue);
    match &event.kind {
        Kind::Claim { issue, .. } | Kind::Work { issue } if !working_on(reading, *issue) => {
            end_work(agent, reading, t, "another issue's work");
            reading.work = Some(Work { issue: *issue, mr: event.kind.mr(), span: Span { start: t, end: t, open: false }, ended: None });
        }
        Kind::Claim { mr: Some(mr), .. } | Kind::Submit { mr: Some(mr), .. } => {
            if let Some(work) = reading.work.as_mut() {
                work.mr = work.mr.or(Some(*mr));
            }
        }
        Kind::Stop { issue, reason } if working_on(reading, *issue) => {
            let why = reason.as_ref().map_or_else(|| "stop".to_owned(), |r| format!("stop ({})", stop_reason(r)));
            end_work(agent, reading, t, &why);
        }
        Kind::Release { issue, .. } if working_on(reading, *issue) => end_work(agent, reading, t, "release"),
        _ => {}
    }
    if !matches!(event.kind, Kind::Work { .. }) {
        mark(agent, event);
    }
}

fn read_sub(agent: &mut Agent, reading: &mut Reading, event: &Event) {
    let t = event.t;
    match &event.kind {
        Kind::SubStart { sub, name, model, .. } => {
            let open = Sub {
                sub: sub.clone(),
                name: name.clone(),
                model: model.clone(),
                span: Span { start: t, end: t, open: false },
                start_note: event.note.clone(),
                stop_note: None,
                rc: None,
                issues: Vec::new(),
                prs: event.note.as_deref().map(pr_refs).unwrap_or_default(),
            };
            reading.subs.insert(sub.clone(), open);
        }
        Kind::SubStop { sub } => {
            if let Some(mut done) = reading.subs.remove(sub) {
                done.span.end = t.max(done.span.start);
                done.rc = event.note.as_deref().and_then(rc_of);
                done.stop_note = event.note.clone();
                agent.subs.push(done);
            }
        }
        _ => {}
    }
}

pub(crate) fn stop_reason(reason: &swarmception_model::StopReason) -> String {
    use swarmception_model::StopReason;
    match reason {
        StopReason::Handoff => "handoff".into(),
        StopReason::Blocked => "blocked".into(),
        StopReason::Paused => "paused".into(),
        StopReason::Done => "done".into(),
        StopReason::Other(other) => other.clone(),
    }
}

fn push_unique(list: &mut Vec<u64>, value: u64) {
    if !list.contains(&value) {
        list.push(value);
    }
}

fn mark(agent: &mut Agent, event: &Event) {
    let reason = match &event.kind {
        Kind::Stop { reason, .. } => reason.as_ref().map(stop_reason),
        Kind::Release { reason, .. } | Kind::SessionStop { reason, .. } => reason.clone(),
        _ => None,
    };
    agent.marks.push(Mark {
        t: event.t,
        event: event.kind.name().to_owned(),
        issue: event.kind.issue(),
        mr: event.kind.mr(),
        note: event.note.clone(),
        sub: event.sub.clone(),
        reason,
    });
}

fn end_work(agent: &mut Agent, reading: &mut Reading, t: Timestamp, why: &str) {
    if let Some(mut work) = reading.work.take() {
        work.span.end = t.max(work.span.start);
        work.ended = Some(why.to_owned());
        agent.work.push(work);
    }
}

fn end_session(agent: &mut Agent, reading: &mut Reading, t: Timestamp, by: Option<&str>, open: bool) {
    if let Some(work) = reading.work.as_mut() {
        work.span.open = open;
    }
    end_work(agent, reading, t, if open { "nothing yet" } else { "session end" });
    if let Some(mut session) = reading.session.take() {
        if let Some(since) = reading.pause.take() {
            session.pauses.push(Span { start: since, end: t.max(since), open });
        }
        session.span.end = t.max(session.span.start);
        session.span.open = open;
        session.ended_by = by.map(str::to_owned);
        agent.sessions.push(session);
    }
}

/// What one instance's events end of another's: a coordinator's `session.stop` with `of` ends
/// the named instance's open session there, and a `release` with `of` its work on the issue.
fn apply_others(agents: &mut [Agent], files: &[AgentFile]) {
    for file in files {
        for event in &file.events {
            let (of, issue) = match &event.kind {
                Kind::SessionStop { of: Some(of), .. } => (of, None),
                Kind::Release { of: Some(of), issue, .. } => (of, Some(*issue)),
                _ => continue,
            };
            let Some(target) = agents.iter_mut().find(|a| &a.instance == of) else { continue };
            match issue {
                None => {
                    for session in target.sessions.iter_mut().filter(|s| s.span.open && s.span.start <= event.t) {
                        session.span.end = event.t;
                        session.span.open = false;
                        session.ended_by = Some(file.instance.clone());
                        for pause in session.pauses.iter_mut().filter(|p| p.open) {
                            pause.end = event.t.max(pause.start);
                            pause.open = false;
                        }
                    }
                    for work in target.work.iter_mut().filter(|w| w.span.open && w.span.start <= event.t) {
                        work.span.end = event.t;
                        work.span.open = false;
                        work.ended = Some(format!("session ended by {}", file.instance));
                    }
                }
                Some(issue) => {
                    for work in target.work.iter_mut().filter(|w| w.issue == issue && w.span.open && w.span.start <= event.t) {
                        work.span.end = event.t;
                        work.span.open = false;
                        work.ended = Some(format!("released by {}", file.instance));
                    }
                }
            }
        }
    }
}

/// The exit code a stop note names first: `rc=<n>`.
#[must_use]
pub fn rc_of(note: &str) -> Option<i64> {
    let rest = note.trim_start().strip_prefix("rc=")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    digits.parse().ok()
}

/// Every pull request a note names, `#<m>`, in order, each once.
#[must_use]
pub fn pr_refs(note: &str) -> Vec<u64> {
    let mut refs = Vec::new();
    let mut rest = note;
    while let Some(at) = rest.find('#') {
        rest = rest.get(at + 1..).unwrap_or_default();
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse() {
            push_unique(&mut refs, n);
        }
    }
    refs
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use swarmception_model::parse_file;

    pub(crate) fn sample() -> Vec<AgentFile> {
        let worker = r#"{"effort":"high","event":"session.start","format":3,"machine":"mini","model":"claude-opus-5-5","name":"worker-2","role":"worker","t":"2026-10-31T03:53:06Z"}
{"event":"claim","issue":170,"mr":173,"t":"2026-10-31T03:53:22Z"}
{"event":"work","issue":170,"t":"2026-10-31T03:53:25Z"}
{"event":"sub.start","name":"ready","note":"ci --changed before submitting","sub":"ready-4bb1de4","t":"2026-10-31T04:06:00Z"}
{"event":"sub.stop","note":"rc=0","sub":"ready-4bb1de4","t":"2026-10-31T04:06:37Z"}
{"event":"submit","issue":170,"mr":173,"t":"2026-10-31T04:06:43Z"}
{"event":"session.pause","t":"2026-10-31T04:10:00Z"}
{"event":"session.resume","t":"2026-10-31T04:12:00Z"}
{"event":"stop","issue":170,"reason":"done","t":"2026-10-31T04:14:00Z"}
{"event":"session.stop","note":"no unclaimed issues left","t":"2026-10-31T04:17:04Z"}
{"event":"session.stop","note":"no unclaimed issues left","t":"2026-10-31T04:48:01Z"}"#;
        let coordinator = r#"{"event":"session.start","format":3,"machine":"mini","name":"coordinator","role":"coordinator","t":"2026-10-31T04:00:00Z"}
{"event":"sub.start","name":"gate","note":"local gate: #173 (#170); capacity=3 free=15.0; kind=smoke","sub":"gate-pr173","t":"2026-10-31T04:20:00Z"}
{"event":"merge","issue":170,"mr":173,"sub":"gate-pr173","t":"2026-10-31T04:28:58Z"}
{"event":"sub.stop","note":"rc=0; kind=smoke; merged #173","sub":"gate-pr173","t":"2026-10-31T04:29:02Z"}
{"event":"session.stop","of":"mini-dead01","note":"its machine rebooted","t":"2026-10-31T05:00:00Z"}"#;
        let dead = r#"{"event":"session.start","machine":"mini","name":"worker-1","role":"worker","t":"2026-10-31T03:00:00Z"}
{"event":"work","issue":168,"t":"2026-10-31T03:01:00Z"}"#;
        vec![parse_file("mini-79ba63", worker), parse_file("mini-8d44f3", coordinator), parse_file("mini-dead01", dead)]
    }

    #[test]
    fn sessions_work_and_subs() {
        let activity = Activity::of(&sample());
        assert_eq!(activity.events(), 18);
        let names: Vec<&str> = activity.agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["coordinator", "worker-1", "worker-2"]);
        let worker = &activity.agents[2];
        assert_eq!(worker.sessions.len(), 1, "the second stop ends nothing");
        let session = &worker.sessions[0];
        assert_eq!(session.span.end.to_string(), "2026-10-31T04:17:04Z");
        assert_eq!(session.pauses.len(), 1);
        assert_eq!(worker.work.len(), 1);
        assert_eq!(worker.work[0].issue, 170);
        assert_eq!(worker.work[0].mr, Some(173));
        assert_eq!(worker.work[0].span.start.to_string(), "2026-10-31T03:53:22Z");
        assert_eq!(worker.work[0].ended.as_deref(), Some("stop (done)"));
        assert_eq!(worker.subs[0].rc, Some(0));
        let marks: Vec<&str> = worker.marks.iter().map(|m| m.event.as_str()).collect();
        assert_eq!(marks, ["claim", "submit", "stop"]);
    }

    #[test]
    fn a_gates_issue_events_are_its_own() {
        let activity = Activity::of(&sample());
        let coordinator = &activity.agents[0];
        assert!(coordinator.work.is_empty());
        let gate = &coordinator.subs[0];
        assert!(gate.is_gate());
        assert_eq!(gate.issues, [170]);
        assert_eq!(gate.prs, [173, 170]);
        assert_eq!(coordinator.marks.iter().map(|m| m.event.as_str()).collect::<Vec<_>>(), ["merge", "session.stop"]);
        assert_eq!(activity.prs().get(&170), Some(&173));
    }

    #[test]
    fn a_coordinator_ends_a_dead_agents_session() {
        let activity = Activity::of(&sample());
        let dead = &activity.agents[1];
        assert_eq!(dead.sessions[0].span.end.to_string(), "2026-10-31T05:00:00Z");
        assert!(!dead.sessions[0].span.open);
        assert_eq!(dead.sessions[0].ended_by.as_deref(), Some("mini-8d44f3"));
        assert!(!dead.work[0].span.open);
    }

    #[test]
    fn notes() {
        assert_eq!(rc_of("rc=101; cause=toolchain"), Some(101));
        assert_eq!(rc_of("superseded: x"), None);
        assert_eq!(pr_refs("local gate: #154 (#141) #154 #"), [154, 141]);
    }

    #[test]
    fn nothing_reads_as_nothing() {
        let activity = Activity::of(&[]);
        assert!(activity.agents.is_empty());
        assert_eq!(activity.extent, None);
    }
}
