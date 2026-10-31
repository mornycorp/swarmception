//! The rows the timeline shows (the owner's directions): by worker (machine, then the worker's
//! name, then its sessions, issues, sub-agents and events) or by ticket (the issue, then its
//! work, sub-agents and events), as that tree or flat (the same leaves at the top, named by
//! their path), and collapsed (no empty rows) or not. Each row's bars and pins carry their
//! label, their style and their tooltip lines: what the timeline draws is what this answers.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{Role, Timestamp};

use crate::activity::{Activity, Agent, Mark, Session, Span, Sub, Work};

/// What the rows are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RowsBy {
    /// A row per worker: machine / worker / its sessions, issues, sub-agents and events.
    #[default]
    Workers,
    /// A row per ticket (issue): its work, sub-agents and events.
    Tickets,
}

/// How the rows are grouped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    /// A tree: machine / worker / … or ticket / ….
    #[default]
    Tree,
    /// Every leaf at the top, named by its path.
    Flat,
}

/// Which rows, grouped how.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RowsSpec {
    pub by: RowsBy,
    pub grouping: Grouping,
    /// Rows with nothing in them, and groups left with no rows, are left out.
    pub collapse: bool,
}

/// How a bar or pin is drawn: the host picks its colour by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    WorkerSession,
    CoordinatorSession,
    OtherSession,
    Paused,
    Work,
    Gate,
    GateFailed,
    GateStopped,
    Ready,
    ReadyFailed,
    Sub,
    Claim,
    Submit,
    Merge,
    Sendback,
    Release,
    Stop,
    Other,
}

/// A span on a Ranges row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Bar {
    pub start: Timestamp,
    pub end: Timestamp,
    pub label: String,
    pub style: Style,
    /// Nothing ended it: it runs to the last event.
    pub open: bool,
    pub tooltip: Vec<String>,
}

/// An instant on an Instants row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Pin {
    pub at: Timestamp,
    pub label: String,
    pub style: Style,
    pub tooltip: Vec<String>,
}

/// A row of the tree: a group, or a leaf of bars or pins. Keys are unique and stable across
/// loads of the same swarm, so a row's state (collapsed, pinned) survives a reload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Node {
    Group { key: String, name: String, children: Vec<Node> },
    Ranges { key: String, name: String, bars: Vec<Bar> },
    Instants { key: String, name: String, pins: Vec<Pin> },
}

impl Node {
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Self::Group { key, .. } | Self::Ranges { key, .. } | Self::Instants { key, .. } => key,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Group { name, .. } | Self::Ranges { name, .. } | Self::Instants { name, .. } => name,
        }
    }

    /// Whether it holds nothing to draw: a leaf with no bars or pins, a group whose rows all are.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Group { children, .. } => children.iter().all(Self::is_empty),
            Self::Ranges { bars, .. } => bars.is_empty(),
            Self::Instants { pins, .. } => pins.is_empty(),
        }
    }

    /// Every leaf under it (itself, for a leaf), top to bottom.
    #[must_use]
    pub fn leaves(&self) -> Vec<&Self> {
        match self {
            Self::Group { children, .. } => children.iter().flat_map(Self::leaves).collect(),
            leaf => vec![leaf],
        }
    }
}

/// The rows of `activity` as `spec` asks.
#[must_use]
pub fn rows(activity: &Activity, spec: RowsSpec) -> Vec<Node> {
    let tree = match spec.by {
        RowsBy::Workers => by_workers(activity),
        RowsBy::Tickets => by_tickets(activity),
    };
    let rows = match spec.grouping {
        Grouping::Tree => tree,
        Grouping::Flat => flat(tree, &[]),
    };
    if spec.collapse { collapsed(rows) } else { rows }
}

/// The leaves of `nodes`, at the top, each named by its path.
fn flat(nodes: Vec<Node>, path: &[String]) -> Vec<Node> {
    let named = |name: String| if path.is_empty() { name } else { format!("{} / {name}", path.join(" / ")) };
    nodes
        .into_iter()
        .flat_map(|node| match node {
            Node::Group { name, children, .. } => {
                let mut inner = path.to_vec();
                inner.push(name);
                flat(children, &inner)
            }
            Node::Ranges { key, name, bars } => vec![Node::Ranges { key, name: named(name), bars }],
            Node::Instants { key, name, pins } => vec![Node::Instants { key, name: named(name), pins }],
        })
        .collect()
}

fn collapsed(nodes: Vec<Node>) -> Vec<Node> {
    nodes
        .into_iter()
        .filter_map(|node| match node {
            Node::Group { key, name, children } => {
                let children = collapsed(children);
                (!children.is_empty()).then_some(Node::Group { key, name, children })
            }
            leaf => (!leaf.is_empty()).then_some(leaf),
        })
        .collect()
}

fn by_workers(activity: &Activity) -> Vec<Node> {
    let mut machines: BTreeMap<&str, BTreeMap<&str, Vec<&Agent>>> = BTreeMap::new();
    for agent in &activity.agents {
        machines.entry(&agent.machine).or_default().entry(&agent.name).or_default().push(agent);
    }
    machines
        .into_iter()
        .map(|(machine, workers)| {
            let children = workers.into_iter().map(|(name, agents)| worker(machine, name, &agents)).collect();
            Node::Group { key: format!("w/{machine}"), name: machine.to_owned(), children }
        })
        .collect()
}

fn worker(machine: &str, name: &str, agents: &[&Agent]) -> Node {
    let key = format!("w/{machine}/{name}");
    let mut sessions = Vec::new();
    let mut issues = Vec::new();
    let mut subs = Vec::new();
    let mut pins = Vec::new();
    for agent in agents {
        for session in &agent.sessions {
            sessions.extend(session_bars(agent, session));
        }
        issues.extend(agent.work.iter().map(|w| work_bar(agent, w, format!("#{}", w.issue))));
        subs.extend(agent.subs.iter().map(|s| sub_bar(agent, s)));
        pins.extend(agent.marks.iter().map(|m| pin(agent, m)));
    }
    let children = vec![
        Node::Ranges { key: format!("{key}/sessions"), name: "Sessions".into(), bars: sorted(sessions) },
        Node::Ranges { key: format!("{key}/issues"), name: "Issues".into(), bars: sorted(issues) },
        Node::Ranges { key: format!("{key}/subs"), name: "Sub-agents".into(), bars: sorted(subs) },
        Node::Instants { key: format!("{key}/events"), name: "Events".into(), pins: sorted_pins(pins) },
    ];
    Node::Group { key, name: name.to_owned(), children }
}

fn by_tickets(activity: &Activity) -> Vec<Node> {
    #[derive(Default)]
    struct Ticket {
        work: Vec<Bar>,
        subs: Vec<Bar>,
        pins: Vec<Pin>,
    }
    let prs = activity.prs();
    let mut tickets: BTreeMap<u64, Ticket> = BTreeMap::new();
    for agent in &activity.agents {
        for work in &agent.work {
            tickets.entry(work.issue).or_default().work.push(work_bar(agent, work, agent.name.clone()));
        }
        for mark in &agent.marks {
            if let Some(issue) = mark.issue {
                tickets.entry(issue).or_default().pins.push(pin(agent, mark));
            }
        }
    }
    for agent in &activity.agents {
        for sub in &agent.subs {
            let mut issues = sub.issues.clone();
            issues.extend(prs.iter().filter(|(_, mr)| sub.prs.contains(mr)).map(|(issue, _)| *issue));
            issues.sort_unstable();
            issues.dedup();
            for issue in issues {
                if let Some(ticket) = tickets.get_mut(&issue) {
                    ticket.subs.push(sub_bar(agent, sub));
                }
            }
        }
    }
    tickets
        .into_iter()
        .map(|(issue, ticket)| {
            let key = format!("t/{issue}");
            let name = prs.get(&issue).map_or_else(|| format!("#{issue}"), |mr| format!("#{issue} · PR #{mr}"));
            let children = vec![
                Node::Ranges { key: format!("{key}/work"), name: "Work".into(), bars: sorted(ticket.work) },
                Node::Ranges { key: format!("{key}/subs"), name: "Sub-agents".into(), bars: sorted(ticket.subs) },
                Node::Instants { key: format!("{key}/events"), name: "Events".into(), pins: sorted_pins(ticket.pins) },
            ];
            Node::Group { key, name, children }
        })
        .collect()
}

fn sorted(mut bars: Vec<Bar>) -> Vec<Bar> {
    bars.sort_by_key(|b| (b.start, b.end));
    bars
}

fn sorted_pins(mut pins: Vec<Pin>) -> Vec<Pin> {
    pins.sort_by_key(|p| p.at);
    pins
}

/// A duration as a person reads it: `1 h 05 min`, `3 min 20 s`, `12 s`.
#[must_use]
pub fn duration(seconds: i64) -> String {
    let s = seconds.max(0);
    match s {
        0..60 => format!("{s} s"),
        60..3_600 => format!("{} min {:02} s", s / 60, s % 60),
        3_600..86_400 => format!("{} h {:02} min", s / 3_600, (s % 3_600) / 60),
        _ => format!("{} d {:02} h", s / 86_400, (s % 86_400) / 3_600),
    }
}

/// `start → end (duration)`, `start → … (open)` for an open span.
fn when(span: &Span) -> String {
    let length = duration(span.end.0.saturating_sub(span.start.0));
    if span.open { format!("{} → … ({length} so far, never stopped)", span.start) } else { format!("{} → {} ({length})", span.start, span.end) }
}

fn who(agent: &Agent) -> String {
    let role = match &agent.role {
        Role::Worker => "worker",
        Role::Coordinator => "coordinator",
        Role::Other(other) if other.is_empty() => "unknown role",
        Role::Other(other) => other,
    };
    format!("{} ({role}) · {}", agent.name, agent.instance)
}

fn model(agent: &Agent) -> String {
    format!("model {} · effort {}", agent.model.as_deref().unwrap_or("unknown"), agent.effort.as_deref().unwrap_or("unknown"))
}

/// A session's bars: its active stretches between its pauses, and the pauses.
fn session_bars(agent: &Agent, session: &Session) -> Vec<Bar> {
    let style = match agent.role {
        Role::Worker => Style::WorkerSession,
        Role::Coordinator => Style::CoordinatorSession,
        Role::Other(_) => Style::OtherSession,
    };
    let mut tooltip = vec![who(agent), model(agent), when(&session.span)];
    if let Some(parent) = &agent.parent {
        tooltip.push(format!("started by {parent}"));
    }
    if let Some(by) = &session.ended_by {
        tooltip.push(format!("ended by {by}"));
    }
    tooltip.extend(session.notes.iter().cloned());
    let mut bars = Vec::new();
    let mut at = session.span.start;
    let mut pauses = session.pauses.clone();
    pauses.sort_by_key(|p| p.start);
    for pause in pauses.iter().filter(|p| p.start >= session.span.start && p.end <= session.span.end) {
        if pause.start > at {
            bars.push(Bar { start: at, end: pause.start, label: agent.name.clone(), style, open: false, tooltip: tooltip.clone() });
        }
        let paused = vec![format!("{} paused", agent.name), when(pause)];
        bars.push(Bar { start: pause.start, end: pause.end, label: "paused".into(), style: Style::Paused, open: pause.open, tooltip: paused });
        at = pause.end.max(at);
    }
    if at < session.span.end || bars.is_empty() {
        bars.push(Bar { start: at, end: session.span.end, label: agent.name.clone(), style, open: session.span.open, tooltip });
    }
    bars
}

fn work_bar(agent: &Agent, work: &Work, label: String) -> Bar {
    let mut tooltip = vec![match work.mr {
        Some(mr) => format!("#{} · PR #{mr}", work.issue),
        None => format!("#{}", work.issue),
    }];
    tooltip.push(who(agent));
    tooltip.push(when(&work.span));
    if let Some(ended) = &work.ended {
        tooltip.push(format!("ended: {ended}"));
    }
    Bar { start: work.span.start, end: work.span.end, label, style: Style::Work, open: work.span.open, tooltip }
}

fn sub_bar(agent: &Agent, sub: &Sub) -> Bar {
    let ready = sub.sub.starts_with("ready") || sub.name.as_deref() == Some("ready");
    let style = match (sub.is_gate(), ready, sub.rc) {
        (true, _, Some(0)) => Style::Gate,
        (true, _, Some(_)) => Style::GateFailed,
        (true, _, None) => Style::GateStopped,
        (false, true, Some(rc)) if rc != 0 => Style::ReadyFailed,
        (false, true, _) => Style::Ready,
        _ => Style::Sub,
    };
    let label = sub.name.clone().unwrap_or_else(|| sub.sub.clone());
    let mut tooltip = vec![format!("{label} · {}", sub.sub), who(agent), when(&sub.span)];
    if let Some(model) = &sub.model {
        tooltip.push(format!("model {model}"));
    }
    tooltip.extend(sub.start_note.iter().cloned());
    tooltip.extend(sub.stop_note.iter().cloned());
    Bar { start: sub.span.start, end: sub.span.end, label, style, open: sub.span.open, tooltip }
}

fn pin(agent: &Agent, mark: &Mark) -> Pin {
    let style = match mark.event.as_str() {
        "claim" => Style::Claim,
        "submit" => Style::Submit,
        "merge" => Style::Merge,
        "sendback" => Style::Sendback,
        "release" => Style::Release,
        "stop" | "session.stop" => Style::Stop,
        _ => Style::Other,
    };
    let mut label = mark.event.clone();
    if let Some(issue) = mark.issue {
        label = format!("{label} #{issue}");
    }
    let mut tooltip = vec![label.clone()];
    if let Some(mr) = mark.mr {
        tooltip.push(format!("PR #{mr}"));
    }
    tooltip.push(who(agent));
    tooltip.push(mark.t.to_string());
    if let Some(reason) = &mark.reason {
        tooltip.push(format!("reason: {reason}"));
    }
    if let Some(sub) = &mark.sub {
        tooltip.push(format!("by sub-agent {sub}"));
    }
    tooltip.extend(mark.note.iter().cloned());
    Pin { at: mark.t, label, style, tooltip }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::tests::sample;

    fn shape(nodes: &[Node]) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(nodes: &[Node], depth: usize, out: &mut Vec<String>) {
            for node in nodes {
                out.push(format!("{}{}", "  ".repeat(depth), node.name()));
                if let Node::Group { children, .. } = node {
                    walk(children, depth + 1, out);
                }
            }
        }
        walk(nodes, 0, &mut out);
        out
    }

    #[test]
    fn workers_as_a_tree() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, RowsSpec::default());
        assert_eq!(rows.len(), 1, "one machine");
        let names = shape(&rows);
        assert_eq!(names[0], "mini");
        assert_eq!(&names[1..6], ["  coordinator", "    Sessions", "    Issues", "    Sub-agents", "    Events"]);
        assert_eq!(names.len(), 1 + 3 * 5);
    }

    #[test]
    fn collapse_leaves_out_empty_rows() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, RowsSpec { collapse: true, ..RowsSpec::default() });
        let names = shape(&rows);
        // The coordinator has no issues of its own; worker-1 no sub-agents and no events.
        assert_eq!(
            names,
            [
                "mini",
                "  coordinator",
                "    Sessions",
                "    Sub-agents",
                "    Events",
                "  worker-1",
                "    Sessions",
                "    Issues",
                "  worker-2",
                "    Sessions",
                "    Issues",
                "    Sub-agents",
                "    Events"
            ]
        );
    }

    #[test]
    fn flat_names_leaves_by_their_path() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, RowsSpec { grouping: Grouping::Flat, ..RowsSpec::default() });
        assert!(rows.iter().all(|r| !matches!(r, Node::Group { .. })));
        assert_eq!(rows[0].name(), "mini / coordinator / Sessions");
        assert_eq!(rows[0].key(), "w/mini/coordinator/sessions");
    }

    #[test]
    fn tickets() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, RowsSpec { by: RowsBy::Tickets, ..RowsSpec::default() });
        let names = shape(&rows);
        assert_eq!(names, ["#168", "  Work", "  Sub-agents", "  Events", "#170 · PR #173", "  Work", "  Sub-agents", "  Events"]);
        let Node::Group { children, .. } = &rows[1] else { panic!() };
        let Node::Ranges { bars, .. } = &children[1] else { panic!() };
        assert_eq!(bars.len(), 1, "the gate that merged it");
        assert_eq!(bars[0].style, Style::Gate);
        let Node::Instants { pins, .. } = &children[2] else { panic!() };
        assert_eq!(pins.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["claim #170", "submit #170", "stop #170", "merge #170"]);
    }

    #[test]
    fn a_pause_splits_its_session() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, RowsSpec { grouping: Grouping::Flat, ..RowsSpec::default() });
        let Some(Node::Ranges { bars, .. }) = rows.iter().find(|r| r.key() == "w/mini/worker-2/sessions") else { panic!() };
        let styles: Vec<Style> = bars.iter().map(|b| b.style).collect();
        assert_eq!(styles, [Style::WorkerSession, Style::Paused, Style::WorkerSession]);
        assert_eq!(bars[0].end, bars[1].start);
        assert_eq!(bars[1].end, bars[2].start);
    }

    #[test]
    fn durations() {
        assert_eq!(duration(12), "12 s");
        assert_eq!(duration(200), "3 min 20 s");
        assert_eq!(duration(3_900), "1 h 05 min");
        assert_eq!(duration(90_000), "1 d 01 h");
        assert_eq!(duration(-5), "0 s");
    }
}
