//! The rows the timeline shows (the owner's directions): what happened to tickets. At the top,
//! every merge onto `main` (its merge commits), then every ticket's merge, then a row per ticket
//! of its stretches: claimed, working, waiting for merge, sent back, stopped. The tickets are
//! flat, in the order they began, or grouped by the worker that held them last (machine /
//! worker / ticket); collapsed, rows with nothing in them are left out. Each row's bars and pins
//! carry their label, their style and their tooltip lines: what the timeline draws is what this
//! answers.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{MainMerge, Timestamp};

use crate::activity::Activity;
use crate::tickets::{Phase, Stretch, Ticket, tickets};

/// How the ticket rows are grouped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    /// A row per ticket, in the order they began.
    #[default]
    Flat,
    /// Machine / worker / ticket, by the worker that held each ticket last.
    Workers,
}

/// Grouped how, collapsed or not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RowsSpec {
    pub grouping: Grouping,
    /// Rows with nothing in them, and groups left with no rows, are left out.
    pub collapse: bool,
}

/// How a bar or pin is drawn: the host picks its colour by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    /// A merge commit on main.
    MainMerge,
    /// A ticket's merge.
    Merged,
    Claimed,
    Working,
    Waiting,
    SentBack,
    Stopped,
}

impl From<Phase> for Style {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Claimed => Self::Claimed,
            Phase::Working => Self::Working,
            Phase::Waiting => Self::Waiting,
            Phase::SentBack => Self::SentBack,
            Phase::Stopped => Self::Stopped,
        }
    }
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

/// The keys of the two rows at the top.
pub const MAIN_KEY: &str = "main";
pub const MERGED_KEY: &str = "merged";

/// The rows of `activity` and the merges onto main, as `spec` asks.
#[must_use]
pub fn rows(activity: &Activity, merges: &[MainMerge], spec: RowsSpec) -> Vec<Node> {
    let tickets = tickets(activity);
    let mut rows = vec![
        Node::Instants { key: MAIN_KEY.into(), name: "Merges to main".into(), pins: merges.iter().map(|m| main_pin(m, &tickets)).collect() },
        Node::Instants { key: MERGED_KEY.into(), name: "Tickets merged".into(), pins: merged_pins(&tickets) },
    ];
    match spec.grouping {
        Grouping::Flat => rows.extend(tickets.iter().map(ticket_row)),
        Grouping::Workers => rows.extend(by_workers(activity, &tickets)),
    }
    if spec.collapse { collapsed(rows) } else { rows }
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

/// Machine / worker / ticket; tickets no worker held go last, under "no worker".
fn by_workers(activity: &Activity, tickets: &[Ticket]) -> Vec<Node> {
    let mut machines: BTreeMap<String, BTreeMap<String, Vec<Node>>> = BTreeMap::new();
    let mut unheld = Vec::new();
    for ticket in tickets {
        match ticket.worker.as_deref().and_then(|w| activity.agent(w)) {
            Some(agent) => machines.entry(agent.machine.clone()).or_default().entry(agent.name.clone()).or_default().push(ticket_row(ticket)),
            None => unheld.push(ticket_row(ticket)),
        }
    }
    let mut groups: Vec<Node> = machines
        .into_iter()
        .map(|(machine, workers)| {
            let children = workers.into_iter().map(|(name, rows)| Node::Group { key: format!("w/{machine}/{name}"), name, children: rows }).collect();
            Node::Group { key: format!("w/{machine}"), name: machine, children }
        })
        .collect();
    if !unheld.is_empty() {
        groups.push(Node::Group { key: "w/-".into(), name: "no worker".into(), children: unheld });
    }
    groups
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

fn between(start: Timestamp, end: Timestamp) -> String {
    duration(end.0.saturating_sub(start.0))
}

/// The ticket's four times, those it has: claimed, began, finished, merged.
fn milestones(ticket: &Ticket) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = |what: &str, t: Option<Timestamp>| {
        if let Some(t) = t {
            lines.push(format!("{what:<9} {t}"));
        }
    };
    line("claimed", ticket.claimed);
    line("began", ticket.began);
    line("finished", ticket.finished);
    line("merged", ticket.merged.as_ref().map(|m| m.t));
    if let (Some(from), Some(merged)) = (ticket.claimed.or(ticket.began), &ticket.merged) {
        lines.push(format!("claim to merge: {}", between(from, merged.t)));
    }
    lines
}

fn ticket_row(ticket: &Ticket) -> Node {
    let bars = ticket.stretches.iter().map(|s| stretch_bar(ticket, s)).collect();
    Node::Ranges { key: format!("t/{}", ticket.issue), name: ticket.title(), bars }
}

fn stretch_bar(ticket: &Ticket, stretch: &Stretch) -> Bar {
    let phase = stretch.phase.name();
    let span = if stretch.open {
        format!("{phase}: {} → … ({} so far)", stretch.start, between(stretch.start, stretch.end))
    } else {
        format!("{phase}: {} → {} ({})", stretch.start, stretch.end, between(stretch.start, stretch.end))
    };
    let mut tooltip = vec![ticket.title(), span, format!("by {}", stretch.by)];
    tooltip.extend(stretch.note.iter().cloned());
    tooltip.push(String::new());
    tooltip.extend(milestones(ticket));
    Bar { start: stretch.start, end: stretch.end, label: phase.to_owned(), style: stretch.phase.into(), open: stretch.open, tooltip }
}

fn merged_pins(tickets: &[Ticket]) -> Vec<Pin> {
    let mut pins: Vec<Pin> = tickets
        .iter()
        .filter_map(|ticket| {
            let merged = ticket.merged.as_ref()?;
            let mut tooltip = vec![format!("{} merged", ticket.title()), merged.t.to_string()];
            let by = merged.sub.as_ref().map_or_else(|| format!("by {}", merged.by), |sub| format!("by {} in {sub}", merged.by));
            tooltip.push(by);
            tooltip.push(String::new());
            tooltip.extend(milestones(ticket));
            Some(Pin { at: merged.t, label: format!("#{}", ticket.issue), style: Style::Merged, tooltip })
        })
        .collect();
    pins.sort_by_key(|p| p.at);
    pins
}

fn main_pin(merge: &MainMerge, tickets: &[Ticket]) -> Pin {
    let short = merge.commit.get(..8).unwrap_or(&merge.commit);
    let label = merge.pr.map_or_else(|| short.to_owned(), |pr| format!("PR #{pr}"));
    let mut tooltip = vec![merge.subject.clone(), format!("{short} · {}", merge.t)];
    if let Some(pr) = merge.pr {
        let closed: Vec<String> = tickets.iter().filter(|t| t.merged.as_ref().is_some_and(|m| m.pr == Some(pr))).map(|t| format!("#{}", t.issue)).collect();
        tooltip.push(if closed.is_empty() { "no ticket's merge names it".to_owned() } else { format!("merged {}", closed.join(", ")) });
    }
    Pin { at: merge.t, label, style: Style::MainMerge, tooltip }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::tests::sample;
    use swarmception_model::parse_merge;

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

    fn merges() -> Vec<MainMerge> {
        [
            "be043bc0\u{1f}1793420113\u{1f}Merge pull request #173 from mornycorp/170-spec",
            "15bad2c0\u{1f}1793420255\u{1f}Merge pull request #174 from mornycorp/progress-7",
        ]
        .into_iter()
        .filter_map(parse_merge)
        .collect()
    }

    #[test]
    fn main_then_merged_then_a_row_a_ticket() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &merges(), RowsSpec::default());
        assert_eq!(shape(&rows), ["Merges to main", "Tickets merged", "#168", "#170 · PR #173"]);
        let Node::Instants { pins, .. } = &rows[0] else { panic!() };
        assert_eq!(pins.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["PR #173", "PR #174"]);
        assert_eq!(pins[0].tooltip[2], "merged #170");
        assert_eq!(pins[1].tooltip[2], "no ticket's merge names it");
        let Node::Instants { pins, .. } = &rows[1] else { panic!() };
        assert_eq!(pins.iter().map(|p| (p.label.as_str(), p.at.to_string())).collect::<Vec<_>>(), [("#170", "2026-10-31T04:28:58Z".to_owned())]);
        assert!(pins[0].tooltip.contains(&"claim to merge: 35 min 36 s".to_owned()), "{:?}", pins[0].tooltip);
    }

    #[test]
    fn a_tickets_bars_are_its_stretches() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec::default());
        let Node::Ranges { bars, key, .. } = &rows[3] else { panic!() };
        assert_eq!(key, "t/170");
        assert_eq!(bars.iter().map(|b| b.style).collect::<Vec<_>>(), [Style::Claimed, Style::Working, Style::Waiting]);
        assert_eq!(bars[1].tooltip[1], "working: 2026-10-31T03:53:25Z → 2026-10-31T04:06:43Z (13 min 18 s)");
        assert!(bars[1].tooltip.iter().any(|l| l.starts_with("merged") && l.ends_with("04:28:58Z")), "{:?}", bars[1].tooltip);
    }

    #[test]
    fn grouped_by_worker() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec { grouping: Grouping::Workers, ..RowsSpec::default() });
        assert_eq!(shape(&rows), ["Merges to main", "Tickets merged", "mini", "  worker-1", "    #168", "  worker-2", "    #170 · PR #173"]);
    }

    #[test]
    fn collapse_leaves_out_empty_rows() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec { collapse: true, ..RowsSpec::default() });
        assert_eq!(shape(&rows), ["Tickets merged", "#168", "#170 · PR #173"], "no merges onto main read");
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
