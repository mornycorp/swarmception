//! The rows the timeline shows (the owner's directions): what happened to tickets. At the top,
//! every merge onto `main` (its merge commits), then every ticket's merge, each a span of
//! [`MERGE_SECONDS`] from its time (#15), all on one row (the widget stacks the rare two that
//! overlap); then the tickets' stretches (claimed, working, waiting for merge, sent back,
//! stopped), packed onto shared rows:
//! taken in the order they began, each ticket goes on the first row it doesn't overlap, so
//! tickets that didn't run at once share a row. They are packed all together (flat), or within
//! each worker that held them last (machine / worker / its rows), never across workers;
//! collapsed, rows with nothing in them are left out. Each bar carries its label (it names its
//! ticket), its ticket, its style, its colour ([`crate::colours`]: a ticket's working and its merge
//! in its own colour, the rest fixed) and its tooltip lines: what the timeline draws is what this
//! answers.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{Issues, MainMerge, Timestamp};

use crate::activity::Activity;
use crate::colours::{Rgb, style_colour};
use crate::tickets::{Abandoned, Phase, Stretch, Ticket, tickets};
use crate::titles;

/// How the ticket rows are grouped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    /// Every ticket packed onto shared rows, in the order they began.
    #[default]
    Flat,
    /// Machine / worker / its packed rows, by the worker that held each ticket last.
    Workers,
}

/// Grouped how, collapsed or not, labelled with titles or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RowsSpec {
    pub grouping: Grouping,
    /// Rows with nothing in them, and groups left with no rows, are left out.
    pub collapse: bool,
    /// A ticket's bars are labelled with its title (`#58 Architecture review`) when GitHub's
    /// issues have it, else with its phase (`#58 working`). On by default.
    pub titles: bool,
}

impl Default for RowsSpec {
    fn default() -> Self {
        Self { grouping: Grouping::default(), collapse: false, titles: true }
    }
}

/// What a bar shows; its colour follows from this and its ticket ([`style_colour`]).
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
    /// The ticket it is (a stretch of it, or its merge); none for a merge onto main.
    pub ticket: Option<u64>,
    /// The pull request it names: its ticket's (the one its events name, else the newest on
    /// GitHub that closes it), or a merge onto main's.
    pub pr: Option<u64>,
    pub style: Style,
    /// What it is drawn in: [`style_colour`] of its style and ticket.
    pub colour: Rgb,
    /// Nothing ended it: it runs to the last event.
    pub open: bool,
    pub tooltip: Vec<String>,
}

impl Bar {
    fn new(start: Timestamp, end: Timestamp, label: String, ticket: Option<u64>, style: Style, tooltip: Vec<String>) -> Self {
        Self { start, end, label, ticket, pr: None, style, colour: style_colour(style, ticket), open: false, tooltip }
    }
}

/// A row of the tree: a group, or a leaf of bars. Keys are unique and stable across loads of the
/// same swarm, so a row's state (collapsed, pinned) survives a reload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Node {
    Group { key: String, name: String, children: Vec<Node> },
    Ranges { key: String, name: String, bars: Vec<Bar> },
}

impl Node {
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Self::Group { key, .. } | Self::Ranges { key, .. } => key,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Group { name, .. } | Self::Ranges { name, .. } => name,
        }
    }

    /// Whether it holds nothing to draw: a leaf with no bars, a group whose rows all are.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Group { children, .. } => children.iter().all(Self::is_empty),
            Self::Ranges { bars, .. } => bars.is_empty(),
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
/// The flat view's packed rows are keyed `tickets/1`, `tickets/2`, …
pub const TICKETS_KEY: &str = "tickets";
/// How long a merge's span on the top rows lasts, from its time.
pub const MERGE_SECONDS: i64 = 10;

/// The rows of `activity` and the merges onto main, as `spec` asks, with no issues from GitHub.
#[must_use]
pub fn rows(activity: &Activity, merges: &[MainMerge], spec: RowsSpec) -> Vec<Node> {
    rows_with(activity, merges, spec, &Issues::default())
}

/// The rows, as [`rows`], with what GitHub's `issues` say of the tickets: their titles first in
/// the tooltips, and on the bars' labels when `spec.titles` asks (#17).
#[must_use]
pub fn rows_with(activity: &Activity, merges: &[MainMerge], spec: RowsSpec, issues: &Issues) -> Vec<Node> {
    let tickets = tickets(activity);
    let text = Text { issues, titles: spec.titles };
    let mut rows = vec![
        Node::Ranges { key: MAIN_KEY.into(), name: "Merges to main".into(), bars: main_bars(merges, &tickets, issues) },
        Node::Ranges { key: MERGED_KEY.into(), name: "Tickets merged".into(), bars: merged_bars(&tickets, issues) },
    ];
    match spec.grouping {
        Grouping::Flat => rows.extend(packed(&tickets.iter().collect::<Vec<_>>(), TICKETS_KEY, text)),
        Grouping::Workers => rows.extend(by_workers(activity, &tickets, text)),
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

/// Machine / worker / its packed rows; tickets no worker held go last, under "no worker".
fn by_workers(activity: &Activity, tickets: &[Ticket], text: Text<'_>) -> Vec<Node> {
    let mut machines: BTreeMap<String, BTreeMap<String, Vec<&Ticket>>> = BTreeMap::new();
    let mut unheld = Vec::new();
    for ticket in tickets {
        match ticket.worker.as_deref().and_then(|w| activity.agent(w)) {
            Some(agent) => machines.entry(agent.machine.clone()).or_default().entry(agent.name.clone()).or_default().push(ticket),
            None => unheld.push(ticket),
        }
    }
    let mut groups: Vec<Node> = machines
        .into_iter()
        .map(|(machine, workers)| {
            let children = workers
                .into_iter()
                .map(|(name, held)| {
                    let key = format!("w/{machine}/{name}");
                    Node::Group { children: packed(&held, &key, text), key, name }
                })
                .collect();
            Node::Group { key: format!("w/{machine}"), name: machine, children }
        })
        .collect();
    let unheld = packed(&unheld, "w/-", text);
    if !unheld.is_empty() {
        groups.push(Node::Group { key: "w/-".into(), name: "no worker".into(), children: unheld });
    }
    groups
}

/// What labels and tooltips say besides the events: GitHub's issues, and whether bars carry titles.
#[derive(Clone, Copy)]
struct Text<'a> {
    issues: &'a Issues,
    titles: bool,
}

/// The lines a ticket's tooltips start with: what GitHub says of it (its title, state and labels,
/// its pull request), else `#170 · PR #173`.
fn ticket_lines(ticket: &Ticket, issues: &Issues) -> Vec<String> {
    titles::header(issues, ticket.issue, ticket.pr, None).unwrap_or_else(|| vec![ticket.title()])
}

/// The ticket's pull request: the one its events name, else the newest on GitHub that closes it.
fn ticket_pr(ticket: &Ticket, issues: &Issues) -> Option<u64> {
    ticket.pr.or_else(|| issues.pr_of(ticket.issue, None).map(|pr| pr.number))
}

/// Where a ticket's bars run: its first stretch's start to its last one's end. `touches` when
/// its last bar is an instant, which the timeline draws over the item at its end, so a ticket
/// starting then would overlap it.
struct Extent {
    start: Timestamp,
    end: Timestamp,
    touches: bool,
}

impl Extent {
    fn of(ticket: &Ticket) -> Option<Self> {
        let (first, last) = (ticket.stretches.first()?, ticket.stretches.last()?);
        Some(Self { start: first.start, end: last.end, touches: last.start == last.end })
    }

    /// Whether `next`, which starts no earlier, clashes with this.
    fn clashes(&self, next: &Self) -> bool {
        next.start < self.end || (next.start == self.end && self.touches)
    }
}

/// The tickets packed onto rows `<key>/1`, `<key>/2`, …: taken in the order their bars start,
/// each on the first row whose last ticket it doesn't overlap, a new row only when none fits
/// (first fit by start, the fewest rows for intervals). A later ticket never moves an earlier
/// one, so the rows keep their keys as the swarm goes on. Tickets with no stretches have no bars
/// and take no row.
fn packed(tickets: &[&Ticket], key: &str, text: Text<'_>) -> Vec<Node> {
    let mut placed: Vec<(Extent, &Ticket)> = tickets.iter().filter_map(|t| Extent::of(t).map(|e| (e, *t))).collect();
    placed.sort_by_key(|(e, t)| (e.start, t.issue));
    let mut rows: Vec<(Extent, Vec<Bar>)> = Vec::new();
    for (extent, ticket) in placed {
        let bars = ticket.stretches.iter().map(|s| stretch_bar(ticket, s, text));
        match rows.iter_mut().find(|(last, _)| !last.clashes(&extent)) {
            Some(row) => {
                row.1.extend(bars);
                row.0 = extent;
            }
            None => rows.push((extent, bars.collect())),
        }
    }
    rows.into_iter().zip(1..).map(|((_, bars), n)| Node::Ranges { key: format!("{key}/{n}"), name: format!("Tickets {n}"), bars }).collect()
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

/// The ticket's times, those it has: claimed, began, finished, merged, done (a `stop done` while
/// worked on), and, done with no merge, that and the stop's note.
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
    line("done", ticket.done.as_ref().map(|d| d.t));
    let from = ticket.claimed.or(ticket.began);
    if let (Some(from), Some(merged)) = (from, &ticket.merged) {
        lines.push(format!("claim to merge: {}", between(from, merged.t)));
    } else if let Some(done) = &ticket.done {
        if let Some(from) = from {
            lines.push(format!("claim to done: {}", between(from, done.t)));
        }
        lines.push(done.note.as_ref().map_or_else(|| "done without a merge".to_owned(), |note| format!("done without a merge: {note}")));
    }
    lines
}

fn stretch_bar(ticket: &Ticket, stretch: &Stretch, text: Text<'_>) -> Bar {
    let phase = stretch.phase.name();
    let span = if stretch.open {
        format!("{phase}: {} → … ({} so far)", stretch.start, between(stretch.start, stretch.end))
    } else {
        format!("{phase}: {} → {} ({})", stretch.start, stretch.end, between(stretch.start, stretch.end))
    };
    let mut tooltip = ticket_lines(ticket, text.issues);
    tooltip.extend([span, format!("by {}", stretch.by)]);
    tooltip.extend(stretch.note.iter().cloned());
    tooltip.extend(stretch.abandoned.as_ref().map(Abandoned::text));
    tooltip.push(String::new());
    tooltip.extend(milestones(ticket));
    let titled = text.titles.then(|| titles::label(text.issues, ticket.issue)).flatten();
    let label = titled.unwrap_or_else(|| format!("#{} {phase}", ticket.issue));
    let pr = ticket_pr(ticket, text.issues);
    Bar { open: stretch.open, pr, ..Bar::new(stretch.start, stretch.end, label, Some(ticket.issue), stretch.phase.into(), tooltip) }
}

/// A merge's span on the top rows: [`MERGE_SECONDS`] from `at`.
fn merge_end(at: Timestamp) -> Timestamp {
    Timestamp(at.0.saturating_add(MERGE_SECONDS))
}

fn merged_bars(tickets: &[Ticket], issues: &Issues) -> Vec<Bar> {
    let mut bars: Vec<Bar> = tickets
        .iter()
        .filter_map(|ticket| {
            let merged = ticket.merged.as_ref()?;
            let mut tooltip = ticket_lines(ticket, issues);
            match tooltip.first_mut() {
                Some(first) if issues.get(ticket.issue).is_none() => first.push_str(" merged"),
                _ => tooltip.push("merged".to_owned()),
            }
            tooltip.push(merged.t.to_string());
            let by = merged.sub.as_ref().map_or_else(|| format!("by {}", merged.by), |sub| format!("by {} in {sub}", merged.by));
            tooltip.push(by);
            tooltip.push(String::new());
            tooltip.extend(milestones(ticket));
            let pr = merged.pr.or_else(|| ticket_pr(ticket, issues));
            Some(Bar { pr, ..Bar::new(merged.t, merge_end(merged.t), format!("#{}", ticket.issue), Some(ticket.issue), Style::Merged, tooltip) })
        })
        .collect();
    bars.sort_by_key(|b| (b.start, b.ticket));
    bars
}

fn main_bars(merges: &[MainMerge], tickets: &[Ticket], issues: &Issues) -> Vec<Bar> {
    let mut bars: Vec<Bar> = merges.iter().map(|m| main_bar(m, tickets, issues)).collect();
    bars.sort_by_key(|b| b.start);
    bars
}

fn main_bar(merge: &MainMerge, tickets: &[Ticket], issues: &Issues) -> Bar {
    let short = merge.commit.get(..8).unwrap_or(&merge.commit);
    let label = merge.pr.map_or_else(|| short.to_owned(), |pr| format!("PR #{pr}"));
    let mut tooltip = vec![merge.subject.clone(), format!("{short} · {}", merge.t)];
    if let Some(pr) = merge.pr
        && let Some(title) = titles::title(issues, pr)
    {
        tooltip.insert(0, format!("PR #{pr} {}", titles::short(title, titles::TOOLTIP_TITLE)));
    }
    if let Some(pr) = merge.pr {
        let closed: Vec<String> = tickets.iter().filter(|t| t.merged.as_ref().is_some_and(|m| m.pr == Some(pr))).map(|t| format!("#{}", t.issue)).collect();
        tooltip.push(if closed.is_empty() { "no ticket's merge names it".to_owned() } else { format!("merged {}", closed.join(", ")) });
    }
    Bar { pr: merge.pr, ..Bar::new(merge.t, merge_end(merge.t), label, None, Style::MainMerge, tooltip) }
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
    fn main_then_merged_then_the_packed_tickets() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &merges(), RowsSpec::default());
        assert_eq!(shape(&rows), ["Merges to main", "Tickets merged", "Tickets 1", "Tickets 2"], "#168 runs on past #170, so they can't share");
        let Node::Ranges { bars, .. } = &rows[0] else { panic!() };
        assert_eq!(bars.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["PR #173", "PR #174"]);
        assert_eq!(bars[0].tooltip[2], "merged #170");
        assert_eq!(bars[1].tooltip[2], "no ticket's merge names it");
        let Node::Ranges { bars, .. } = &rows[1] else { panic!() };
        assert_eq!(bars.iter().map(|b| (b.label.as_str(), b.start.to_string())).collect::<Vec<_>>(), [("#170", "2026-10-31T04:28:58Z".to_owned())]);
        assert!(bars[0].tooltip.contains(&"claim to merge: 35 min 36 s".to_owned()), "{:?}", bars[0].tooltip);
    }

    #[test]
    fn the_top_rows_are_ten_second_spans_main_green_a_tickets_merge_its_own_colour() {
        use crate::colours::{MAIN_MERGE, ticket_colour};
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &merges(), RowsSpec::default());
        let (Node::Ranges { bars: main, .. }, Node::Ranges { bars: merged, .. }) = (&rows[0], &rows[1]) else { panic!() };
        assert_eq!(MERGE_SECONDS, 10);
        for bar in main.iter().chain(merged) {
            assert_eq!(bar.end.0 - bar.start.0, MERGE_SECONDS, "{}", bar.label);
            assert!(!bar.open);
        }
        assert_eq!(
            main.iter().map(|b| (b.start.0, b.ticket, b.style, b.colour)).collect::<Vec<_>>(),
            [(1_793_420_113, None, Style::MainMerge, MAIN_MERGE), (1_793_420_255, None, Style::MainMerge, MAIN_MERGE)]
        );
        assert_eq!(merged.iter().map(|b| (b.ticket, b.style, b.colour)).collect::<Vec<_>>(), [(Some(170), Style::Merged, ticket_colour(170))]);
    }

    #[test]
    fn a_tickets_bars_are_its_stretches_and_name_it() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec::default());
        let Node::Ranges { bars, key, .. } = &rows[3] else { panic!() };
        assert_eq!(key, "tickets/2");
        assert_eq!(bars.iter().map(|b| b.style).collect::<Vec<_>>(), [Style::Claimed, Style::Working, Style::Waiting]);
        assert!(bars.iter().all(|b| b.ticket == Some(170)), "each bar says which ticket it is");
        let colours: Vec<_> = bars.iter().map(|b| b.colour).collect();
        assert_eq!(colours, [crate::colours::CLAIMED, crate::colours::ticket_colour(170), crate::colours::WAITING], "only working is the ticket's own");
        assert_eq!(bars.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["#170 claimed", "#170 working", "#170 waiting for merge"]);
        assert_eq!(bars[1].tooltip[0], "#170 · PR #173");
        assert_eq!(bars[1].tooltip[1], "working: 2026-10-31T03:53:25Z → 2026-10-31T04:06:43Z (13 min 18 s)");
        assert!(bars[1].tooltip.iter().any(|l| l.starts_with("merged") && l.ends_with("04:28:58Z")), "{:?}", bars[1].tooltip);
    }

    #[test]
    fn an_abandoned_bar_ends_at_its_holders_session_and_says_so() {
        let activity = Activity::of(&crate::tickets::tests::abandoned());
        let rows = rows(&activity, &[], RowsSpec::default());
        let bars: Vec<&Bar> = rows
            .iter()
            .flat_map(|n| n.leaves())
            .flat_map(|n| match n {
                Node::Ranges { bars, .. } => bars.iter().collect::<Vec<_>>(),
                Node::Group { .. } => Vec::new(),
            })
            .filter(|b| b.ticket == Some(40) && b.style == Style::Working)
            .collect();
        assert_eq!(bars.len(), 2, "worker-1's, abandoned, and worker-3's");
        assert!(!bars[0].open);
        assert_eq!(bars[0].tooltip[1], "working: 2026-10-31T01:00:15Z → 2026-10-31T01:10:00Z (9 min 45 s)");
        assert_eq!(bars[0].tooltip[3], "abandoned: m-1's session ended 2026-10-31T01:10:00Z");
        assert!(!bars[1].tooltip.iter().any(|l| l.starts_with("abandoned")), "{:?}", bars[1].tooltip);
    }

    #[test]
    fn with_githubs_issues_a_tooltip_starts_with_the_title_then_state_and_labels() {
        let activity = Activity::of(&sample());
        let issues = crate::titles::tests::issues();
        let rows = rows_with(&activity, &merges(), RowsSpec::default(), &issues);
        let Node::Ranges { bars, .. } = &rows[3] else { panic!() };
        let working = &bars[1];
        assert_eq!(
            working.tooltip[..4],
            [
                "#170 The spec, written down",
                "closed · type::task, area::docs · by cmaughan",
                "PR #173 Spec: the first draft (merged)",
                "working: 2026-10-31T03:53:25Z → 2026-10-31T04:06:43Z (13 min 18 s)"
            ]
        );
        assert!(working.tooltip.iter().any(|l| l.starts_with("merged") && l.ends_with("04:28:58Z")), "then today's lines: {:?}", working.tooltip);
        assert_eq!(bars.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["#170 The spec, written down"; 3], "every bar of it carries its title");
        assert!(bars.iter().all(|b| b.pr == Some(173)));
        let Node::Ranges { bars: main, .. } = &rows[0] else { panic!() };
        assert_eq!(main[0].tooltip[0], "PR #173 Spec: the first draft", "a merge onto main says its PR's title");
        assert_eq!((main[0].pr, main[1].pr), (Some(173), Some(174)));
        assert_eq!(main[1].tooltip[0], "Merge pull request #174 from mornycorp/progress-7", "GitHub didn't list #174");
        let Node::Ranges { bars: merged, .. } = &rows[1] else { panic!() };
        assert_eq!(merged[0].tooltip[0], "#170 The spec, written down");
        assert_eq!(merged[0].tooltip[3], "merged");
        assert_eq!(merged[0].label, "#170", "the top rows' spans are too short for titles");
        // Titles off: the labels name the phase again; the tooltips keep the title.
        let rows = rows_with(&activity, &merges(), RowsSpec { titles: false, ..RowsSpec::default() }, &issues);
        let Node::Ranges { bars, .. } = &rows[3] else { panic!() };
        assert_eq!(bars[1].label, "#170 working");
        assert_eq!(bars[1].tooltip[0], "#170 The spec, written down");
    }

    #[test]
    fn a_ticket_github_doesnt_list_keeps_its_tooltip() {
        let activity = Activity::of(&sample());
        let all = crate::titles::tests::issues();
        let issues = Issues::new(all.iter().filter(|i| i.number != 168).cloned().collect());
        let with = rows_with(&activity, &[], RowsSpec::default(), &issues);
        let without = rows(&activity, &[], RowsSpec::default());
        let of = |rows: &[Node]| -> Vec<Bar> {
            rows.iter()
                .flat_map(Node::leaves)
                .filter_map(|l| if let Node::Ranges { bars, .. } = l { Some(bars.clone()) } else { None })
                .flatten()
                .filter(|b| b.ticket == Some(168))
                .collect()
        };
        assert!(!of(&with).is_empty());
        assert_eq!(of(&with), of(&without), "#168's bars are as they were");
    }

    #[test]
    fn rows_spec_reads_back_with_titles_on_by_default() {
        assert!(RowsSpec::default().titles);
        let old: RowsSpec = serde_json::from_str(r#"{"grouping":"workers","collapse":true}"#).unwrap();
        assert_eq!(old, RowsSpec { grouping: Grouping::Workers, collapse: true, titles: true }, "a spec from before #17 reads, titles on");
    }

    #[test]
    fn a_ticket_done_without_a_merge_says_so() {
        let files = [swarmception_model::parse_file("m-1", crate::tickets::tests::DONE)];
        let rows = rows(&Activity::of(&files), &[], RowsSpec::default());
        let bars: Vec<&Bar> =
            rows.iter().flat_map(Node::leaves).filter_map(|l| if let Node::Ranges { bars, .. } = l { Some(bars) } else { None }).flatten().collect();
        let working = bars.iter().find(|b| b.label == "#58 working").unwrap();
        assert!(!working.open);
        assert_eq!(working.tooltip[1], "working: 2026-10-31T12:59:18Z → 2026-10-31T13:06:43Z (7 min 25 s)");
        assert!(working.tooltip.contains(&"done      2026-10-31T13:06:43Z".to_owned()), "{:?}", working.tooltip);
        assert!(working.tooltip.contains(&"claim to done: 7 min 27 s".to_owned()), "{:?}", working.tooltip);
        assert_eq!(working.tooltip.last().map(String::as_str), Some("done without a merge: review filed as #66-#73"));
        let claimed = bars.iter().find(|b| b.label == "#59 claimed").unwrap();
        assert_eq!(claimed.tooltip.last().map(String::as_str), Some("done without a merge"));
    }

    #[test]
    fn grouped_by_worker() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec { grouping: Grouping::Workers, ..RowsSpec::default() });
        assert_eq!(shape(&rows), ["Merges to main", "Tickets merged", "mini", "  worker-1", "    Tickets 1", "  worker-2", "    Tickets 1"]);
        let keys: Vec<&str> = rows.iter().flat_map(Node::leaves).map(Node::key).collect();
        assert_eq!(keys, ["main", "merged", "w/mini/worker-1/1", "w/mini/worker-2/1"]);
    }

    #[test]
    fn collapse_leaves_out_empty_rows() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec { collapse: true, ..RowsSpec::default() });
        assert_eq!(shape(&rows), ["Tickets merged", "Tickets 1", "Tickets 2"], "no merges onto main read");
    }

    /// `worker`'s session on `machine`, then a `work` and a `release` of each ticket: its one
    /// stretch, from `from` to `to` (minutes past 01:00).
    fn worker(instance: &str, machine: &str, name: &str, tickets: &[(u64, u32, u32)]) -> swarmception_model::AgentFile {
        let at = |m: u32| format!("2026-10-31T{:02}:{:02}:00Z", 1 + m / 60, m % 60);
        let mut lines = vec![format!(r#"{{"event":"session.start","format":3,"machine":"{machine}","name":"{name}","role":"worker","t":"{}"}}"#, at(0))];
        for &(issue, from, to) in tickets {
            lines.push(format!(r#"{{"event":"work","issue":{issue},"t":"{}"}}"#, at(from)));
            lines.push(format!(r#"{{"event":"release","issue":{issue},"t":"{}"}}"#, at(to)));
        }
        swarmception_model::parse_file(instance, &lines.join("\n"))
    }

    /// Each leaf's key, and the tickets its bars name, in order.
    fn packing(nodes: &[Node]) -> Vec<(String, Vec<u64>)> {
        nodes
            .iter()
            .flat_map(Node::leaves)
            .filter_map(|leaf| match leaf {
                Node::Ranges { key, bars, .. } if key != MAIN_KEY && key != MERGED_KEY => {
                    let mut issues: Vec<u64> = bars.iter().filter_map(|b| b.ticket).collect();
                    issues.dedup();
                    Some((key.clone(), issues))
                }
                _ => None,
            })
            .collect()
    }

    /// No two tickets on a row overlap: in order, each starts no earlier than the one before ends.
    fn assert_no_overlap(nodes: &[Node]) {
        for leaf in nodes.iter().flat_map(Node::leaves) {
            let Node::Ranges { key, bars, .. } = leaf else { continue };
            if key == MAIN_KEY || key == MERGED_KEY {
                continue;
            }
            let mut spans: Vec<(String, Timestamp, Timestamp)> = Vec::new();
            for bar in bars {
                let issue = bar.label.split(' ').next().unwrap_or_default().to_owned();
                match spans.last_mut() {
                    Some(last) if last.0 == issue => last.2 = bar.end,
                    _ => spans.push((issue, bar.start, bar.end)),
                }
            }
            for pair in spans.windows(2) {
                assert!(pair[1].1 >= pair[0].2, "{key}: {} overlaps {}", pair[1].0, pair[0].0);
            }
        }
    }

    #[test]
    fn first_fit_by_start() {
        // #3 starts as #1 ends, so it shares #1's row; #4 starts as #2 ends; #5 fits on neither.
        let files = [worker("m-1", "mini", "worker-1", &[(1, 0, 10), (2, 5, 20), (3, 10, 30), (4, 20, 25), (5, 12, 15)])];
        let rows = rows(&Activity::of(&files), &[], RowsSpec::default());
        assert_eq!(packing(&rows), [("tickets/1", vec![1, 3]), ("tickets/2", vec![2, 4]), ("tickets/3", vec![5])].map(|(k, v)| (k.to_owned(), v)));
        assert_eq!(shape(&rows)[2..], ["Tickets 1", "Tickets 2", "Tickets 3"]);
        assert_no_overlap(&rows);
    }

    #[test]
    fn an_instant_at_a_rows_end_is_a_clash() {
        // #1 is an instant at 01:40 (its work released at once), drawn over that item: #2,
        // starting then, can't share its row; #3, a minute later, can.
        let files = [worker("m-1", "mini", "worker-1", &[(1, 40, 40), (2, 40, 50), (3, 41, 45)])];
        let rows = rows(&Activity::of(&files), &[], RowsSpec::default());
        assert_eq!(packing(&rows), [("tickets/1", vec![1, 3]), ("tickets/2", vec![2])].map(|(k, v)| (k.to_owned(), v)));
    }

    #[test]
    fn by_worker_packs_within_each_worker_only() {
        // On mini, worker-1's tickets and worker-2's take turns, so flat they share a row; by
        // worker, each worker's rows hold only its own.
        let files = [
            worker("m-1", "mini", "worker-1", &[(1, 0, 10), (3, 20, 30)]),
            worker("m-2", "mini", "worker-2", &[(2, 10, 20), (4, 30, 40), (5, 35, 45)]),
            worker("b-1", "box", "worker-1", &[(6, 0, 50)]),
        ];
        let activity = Activity::of(&files);
        let flat = rows(&activity, &[], RowsSpec::default());
        assert_eq!(packing(&flat), [("tickets/1", vec![1, 2, 3, 4]), ("tickets/2", vec![6]), ("tickets/3", vec![5])].map(|(k, v)| (k.to_owned(), v)));
        let by = rows(&activity, &[], RowsSpec { grouping: Grouping::Workers, ..RowsSpec::default() });
        assert_eq!(
            packing(&by),
            [("w/box/worker-1/1", vec![6]), ("w/mini/worker-1/1", vec![1, 3]), ("w/mini/worker-2/1", vec![2, 4]), ("w/mini/worker-2/2", vec![5])]
                .map(|(k, v)| (k.to_owned(), v))
        );
        assert_eq!(
            shape(&by)[2..],
            ["box", "  worker-1", "    Tickets 1", "mini", "  worker-1", "    Tickets 1", "  worker-2", "    Tickets 1", "    Tickets 2"]
        );
        assert_no_overlap(&flat);
        assert_no_overlap(&by);
    }

    #[test]
    fn keys_hold_across_loads() {
        let before = [worker("m-1", "mini", "worker-1", &[(1, 0, 10), (2, 5, 20), (3, 10, 30)])];
        let after = [worker("m-1", "mini", "worker-1", &[(1, 0, 10), (2, 5, 20), (3, 10, 30), (4, 31, 40), (5, 32, 35)])];
        let load = |files: &[swarmception_model::AgentFile]| rows(&Activity::of(files), &[], RowsSpec::default());
        assert_eq!(load(&before), load(&before), "the same events, the same rows");
        let (before, after) = (packing(&load(&before)), packing(&load(&after)));
        for (key, issues) in &before {
            let later = after.iter().find(|(k, _)| k == key).map(|(_, i)| i);
            assert!(later.is_some_and(|later| later.starts_with(issues)), "{key} held {issues:?}, then {later:?}");
        }
        assert_eq!(after.len(), 2, "#4 and #5 fill the rows there are");
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
