//! The rows the timeline shows (the owner's directions): what happened to tickets. At the top,
//! every merge onto `main` (its merge commits), then every ticket's merge, each a span of
//! [`MERGE_SECONDS`] from its time (#15), all on one row (the widget stacks the rare two that
//! overlap); under them the gate's runs (#84): a row of the coordinator's gate runs and one of
//! the workers' `ready` checks, a bar a run ([`crate::gates`]' one definition, `gates.list`'s
//! runs) from its start to its end (a running one's to the last event, open), in its result's
//! colour, named by the tickets it carried and a failure's cause, its tooltip `gates.list`'s; then
//! the tickets' stretches (claimed, working, waiting for merge, sent back, stopped), packed onto
//! shared rows:
//! taken in the order they began, each ticket goes on the first row it doesn't overlap, so
//! tickets that didn't run at once share a row. They are packed all together (flat), or within
//! each worker that held them last (machine / worker / its rows), never across workers;
//! collapsed, rows with nothing in them are left out. Each bar carries its label (it names its
//! ticket), its ticket, its style, its colour ([`crate::colours`]: a ticket's working and its merge
//! in its own colour, the rest fixed) and its tooltip lines, styled (#35: [`TipLine`], the phases
//! in their colours, merged green, sent back and abandoned red, durations strong) and as plain
//! text: what the timeline draws is what this answers.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{Issues, MainMerge, Timestamp};

use crate::activity::Activity;
use crate::colours::{self, Rgb, phase_colour, style_colour};
use crate::details::Subject;
use crate::gates::{GateResult, Lane, Run, told_runs};
use crate::stats::failure_label;
use crate::tickets::{Abandoned, Phase, Stretch, Ticket, tickets};
use crate::titles;
use crate::tooltip::{FAILED, PASSED, TipLine, texts};

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

/// Which rows Collapse leaves out (#74).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", from = "CollapseWire")]
pub enum Collapse {
    /// Every row, empty or not.
    #[default]
    Off,
    /// Rows with no events at all, and groups left with no rows: the rows stay put while panning.
    Empty,
    /// Rows with nothing in the timeline's visible window as well ([`left_out`]), and groups
    /// left with no rows; the rows follow a pan and a zoom once the window settles.
    Window,
}

/// What `collapse` reads: a mode's name, or the bool it was before #74 (`true` is
/// [`Collapse::Empty`]), so an older saved state or script keeps its choice.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum CollapseWire {
    /// Before #74: `true` leaves out the empty rows.
    Old(bool),
    Mode(CollapseMode),
}

/// [`Collapse`]'s names.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum CollapseMode {
    Off,
    Empty,
    Window,
}

impl From<CollapseWire> for Collapse {
    fn from(wire: CollapseWire) -> Self {
        match wire {
            CollapseWire::Old(false) | CollapseWire::Mode(CollapseMode::Off) => Self::Off,
            CollapseWire::Old(true) | CollapseWire::Mode(CollapseMode::Empty) => Self::Empty,
            CollapseWire::Mode(CollapseMode::Window) => Self::Window,
        }
    }
}

impl JsonSchema for Collapse {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Collapse".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CollapseWire::json_schema(generator)
    }
}

impl Collapse {
    /// Whether it leaves out rows at all.
    #[must_use]
    pub const fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Grouped how, collapsed how. A ticket's bars are always labelled with its title
/// (`#58 Architecture review`) when GitHub's issues have it, else with its phase (`#58 working`):
/// no option (#34).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RowsSpec {
    pub grouping: Grouping,
    /// Which rows are left out: none, those with no events, or those with nothing in the
    /// visible window (#74). `true` and `false` still read, as `empty` and `off`.
    pub collapse: Collapse,
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
    /// A coordinator's gate run (#84), drawn in its result's colour.
    GateRun,
    /// A worker's `ready` check (#84), drawn in its result's colour.
    ReadyCheck,
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
    /// Its tooltip as plain text: [`Bar::tooltip_lines`]' text, a line each.
    pub tooltip: Vec<String>,
    /// Its tooltip's lines with their styling (#35), what the timeline shows.
    pub tooltip_lines: Vec<TipLine>,
    /// The gate run it is, on the Gate runs or Ready checks row (#84); none for any other bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunBar>,
}

impl Bar {
    fn new(start: Timestamp, end: Timestamp, label: String, ticket: Option<u64>, style: Style, lines: Vec<TipLine>) -> Self {
        let colour = style_colour(style, ticket);
        Self { start, end, label, ticket, pr: None, style, colour, open: false, tooltip: texts(&lines), tooltip_lines: lines, run: None }
    }
}

/// The run a bar on the Gate runs or Ready checks row draws (#84): `gates.list`'s run, by its
/// sub-agent's id and its start (the bar's), how it went, what a hover over it lights and what a
/// click on it tells the story of. It names its tickets here, not as its bar's `ticket`: a run is
/// none of them, so a ticket's bars (`timeline.ticketBars`) are its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunBar {
    pub sub: String,
    pub lane: Lane,
    pub result: GateResult,
    /// The tickets it carried: their bars are what a hover over it lights.
    pub tickets: Vec<u64>,
    /// What the details pane tells when it is chosen, as `gates.list`'s run says: the gate run
    /// itself; for a `ready` check, the ticket it checked, else its worker's session.
    pub subject: Subject,
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

/// A bar on the timeline: its row's key and its place among the row's bars (the widget's range).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct BarRef {
    pub track: String,
    pub range: usize,
}

/// The bars of `rows` that `keep` keeps (given its row's key and the bar), top to bottom, each
/// row's left to right.
#[must_use]
pub fn bars_where(rows: &[Node], mut keep: impl FnMut(&str, &Bar) -> bool) -> Vec<(BarRef, &Bar)> {
    let mut found = Vec::new();
    for leaf in rows.iter().flat_map(Node::leaves) {
        if let Node::Ranges { key, bars, .. } = leaf {
            found.extend(bars.iter().enumerate().filter(|(_, bar)| keep(key, bar)).map(|(range, bar)| (BarRef { track: key.clone(), range }, bar)));
        }
    }
    found
}

/// The keys of the groups that hold any of the rows `keys` in `rows`, each once, outermost first
/// in the tree's order (#68: a group collapsed over them opens to show them).
#[must_use]
pub fn groups_holding(rows: &[Node], keys: &[&str]) -> Vec<String> {
    fn walk(node: &Node, keys: &[&str], found: &mut Vec<String>) -> bool {
        match node {
            Node::Ranges { key, .. } => keys.contains(&key.as_str()),
            Node::Group { key, children, .. } => {
                let at = found.len();
                let mut holds = false;
                for child in children {
                    holds |= walk(child, keys, found);
                }
                if holds {
                    found.insert(at, key.clone());
                }
                holds
            }
        }
    }
    let mut found = Vec::new();
    for node in rows {
        walk(node, keys, &mut found);
    }
    found
}

/// The keys of the two rows at the top.
pub const MAIN_KEY: &str = "main";
pub const MERGED_KEY: &str = "merged";
/// The keys of the gate's two rows under them (#84): the coordinator's gate runs, the workers'
/// `ready` checks.
pub const GATES_KEY: &str = "gates";
pub const READY_KEY: &str = "ready";
/// The keys of the rows above the tickets', top to bottom.
pub const TOP_KEYS: [&str; 4] = [MAIN_KEY, MERGED_KEY, GATES_KEY, READY_KEY];
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
/// the tooltips, and on the bars' labels (#17, always since #34).
#[must_use]
pub fn rows_with(activity: &Activity, merges: &[MainMerge], spec: RowsSpec, issues: &Issues) -> Vec<Node> {
    let tickets = tickets(activity);
    let text = Text { issues };
    let runs = told_runs(activity, &tickets, merges);
    let last = activity.extent.map_or(Timestamp(0), |(_, last)| last);
    let mut rows = vec![
        Node::Ranges { key: MAIN_KEY.into(), name: "Merges to main".into(), bars: main_bars(merges, &tickets, issues) },
        Node::Ranges { key: MERGED_KEY.into(), name: "Tickets merged".into(), bars: merged_bars(&tickets, issues) },
        Node::Ranges { key: GATES_KEY.into(), name: "Gate runs".into(), bars: run_bars(&runs, Lane::Gate, last) },
        Node::Ranges { key: READY_KEY.into(), name: "Ready checks".into(), bars: run_bars(&runs, Lane::Ready, last) },
    ];
    match spec.grouping {
        Grouping::Flat => rows.extend(packed(&tickets.iter().collect::<Vec<_>>(), TICKETS_KEY, text)),
        Grouping::Workers => rows.extend(by_workers(activity, &tickets, text)),
    }
    if spec.collapse.is_on() { collapsed(rows) } else { rows }
}

/// Whether `bar` has anything between `start` and `end`, its ends included.
fn overlaps(bar: &Bar, start: Timestamp, end: Timestamp) -> bool {
    bar.start <= end && bar.end >= start
}

/// The keys of the rows of `rows` a window from `start` to `end` leaves out (#74, Collapse to
/// the window): a row of bars none of which overlaps the window, unless its key is in `keep`
/// (a row holding the selection keeps it in view), and a group none of whose rows is left in,
/// in place of its rows (it folds away whole). In the tree's order, each group after the rows
/// under it. The keys stay those of `rows`: what the window leaves out comes back as it was.
#[must_use]
pub fn left_out(rows: &[Node], start: Timestamp, end: Timestamp, keep: &[&str]) -> Vec<String> {
    fn walk(node: &Node, window: (Timestamp, Timestamp), keep: &[&str], out: &mut Vec<String>) -> bool {
        match node {
            Node::Ranges { key, bars, .. } => {
                let shown = keep.contains(&key.as_str()) || bars.iter().any(|bar| overlaps(bar, window.0, window.1));
                if !shown {
                    out.push(key.clone());
                }
                shown
            }
            Node::Group { key, children, .. } => {
                let at = out.len();
                let mut shown = false;
                for child in children {
                    shown |= walk(child, window, keep, out);
                }
                if !shown {
                    out.truncate(at);
                    out.push(key.clone());
                }
                shown
            }
        }
    }
    let mut out = Vec::new();
    for node in rows {
        walk(node, (start, end), keep, &mut out);
    }
    out
}

/// `rows` without the rows and groups `keys` names (what [`left_out`] answers): the rows a
/// window shows.
#[must_use]
pub fn without(rows: &[Node], keys: &[String]) -> Vec<Node> {
    rows.iter()
        .filter(|node| !keys.iter().any(|key| key == node.key()))
        .map(|node| match node {
            Node::Group { key, name, children } => Node::Group { key: key.clone(), name: name.clone(), children: without(children, keys) },
            leaf => leaf.clone(),
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

/// What labels and tooltips say besides the events: GitHub's issues.
#[derive(Clone, Copy)]
struct Text<'a> {
    issues: &'a Issues,
}

/// The lines a ticket's tooltips start with: what GitHub says of it (its title, strong; state and
/// labels; its pull request, merged in green), else `#170 · PR #173`, strong.
fn ticket_lines(ticket: &Ticket, issues: &Issues) -> Vec<TipLine> {
    let header = titles::header(issues, ticket.issue, ticket.pr, None).unwrap_or_else(|| vec![ticket.title()]);
    header.into_iter().enumerate().map(|(n, line)| if n == 0 { TipLine::strong(line) } else { header_line(line) }).collect()
}

/// A header line after the first: plain, but a pull request's `(merged)` in green.
fn header_line(line: String) -> TipLine {
    match line.strip_suffix(" (merged)") {
        Some(before) => TipLine::plain(format!("{before} (")).then_coloured("merged", PASSED).then(")"),
        None => TipLine::plain(line),
    }
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
/// worked on), and, done with no merge, that and the stop's note. Each milestone's name is in the
/// colour of what it begins (claimed yellow, began the ticket's working colour, finished waiting
/// blue, merged green, done grey), the spans between them strong.
fn milestones(ticket: &Ticket) -> Vec<TipLine> {
    let mut lines = Vec::new();
    let mut line = |what: &str, colour: Rgb, t: Option<Timestamp>| {
        if let Some(t) = t {
            let padded = format!("{what:<9}");
            lines.push(TipLine::coloured(what, colour).then(format!("{} {t}", padded.get(what.len()..).unwrap_or_default())));
        }
    };
    line("claimed", colours::CLAIMED, ticket.claimed);
    line("began", phase_colour(Phase::Working, ticket.issue), ticket.began);
    line("finished", colours::WAITING, ticket.finished);
    line("merged", PASSED, ticket.merged.as_ref().map(|m| m.t));
    line("done", colours::STOPPED, ticket.done.as_ref().map(|d| d.t));
    let from = ticket.claimed.or(ticket.began);
    if let (Some(from), Some(merged)) = (from, &ticket.merged) {
        lines.push(TipLine::plain("claim to merge: ").then_strong(between(from, merged.t)));
    } else if let Some(done) = &ticket.done {
        if let Some(from) = from {
            lines.push(TipLine::plain("claim to done: ").then_strong(between(from, done.t)));
        }
        let without = TipLine::coloured("done without a merge", colours::STOPPED);
        lines.push(match &done.note {
            Some(note) => without.then(format!(": {note}")),
            None => without,
        });
    }
    lines
}

/// A stretch's own line: its phase in its colour, its times, its duration strong.
fn span_line(ticket: &Ticket, stretch: &Stretch) -> TipLine {
    let phase = TipLine::coloured(stretch.phase.name(), phase_colour(stretch.phase, ticket.issue));
    let took = between(stretch.start, stretch.end);
    if stretch.open {
        phase.then(format!(": {} → … (", stretch.start)).then_strong(took).then(" so far)")
    } else {
        phase.then(format!(": {} → {} (", stretch.start, stretch.end)).then_strong(took).then(")")
    }
}

/// `abandoned: …`, the word in red.
fn abandoned_line(abandoned: &Abandoned) -> TipLine {
    let text = abandoned.text();
    match text.strip_prefix("abandoned") {
        Some(rest) => TipLine::coloured("abandoned", FAILED).then(rest),
        None => TipLine::plain(text),
    }
}

/// A stretch's note: a sent back stretch's (why: the gate's cause) in red, any other plain.
fn note_line(stretch: &Stretch, note: &str) -> TipLine {
    if stretch.phase == Phase::SentBack { TipLine::coloured(note, FAILED) } else { TipLine::plain(note) }
}

fn stretch_bar(ticket: &Ticket, stretch: &Stretch, text: Text<'_>) -> Bar {
    let phase = stretch.phase.name();
    let mut tooltip = ticket_lines(ticket, text.issues);
    tooltip.extend([span_line(ticket, stretch), TipLine::plain(format!("by {}", stretch.by))]);
    tooltip.extend(stretch.note.iter().map(|note| note_line(stretch, note)));
    tooltip.extend(stretch.abandoned.as_ref().map(abandoned_line));
    tooltip.push(TipLine::default());
    tooltip.extend(milestones(ticket));
    let label = titles::label(text.issues, ticket.issue).unwrap_or_else(|| format!("#{} {phase}", ticket.issue));
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
                Some(first) if issues.get(ticket.issue).is_none() => *first = std::mem::take(first).then(" ").then_coloured("merged", PASSED),
                _ => tooltip.push(TipLine::coloured("merged", PASSED)),
            }
            tooltip.push(TipLine::plain(merged.t.to_string()));
            let by = merged.sub.as_ref().map_or_else(|| format!("by {}", merged.by), |sub| format!("by {} in {sub}", merged.by));
            tooltip.push(TipLine::plain(by));
            tooltip.push(TipLine::default());
            tooltip.extend(milestones(ticket));
            let pr = merged.pr.or_else(|| ticket_pr(ticket, issues));
            Some(Bar { pr, ..Bar::new(merged.t, merge_end(merged.t), format!("#{}", ticket.issue), Some(ticket.issue), Style::Merged, tooltip) })
        })
        .collect();
    bars.sort_by_key(|b| (b.start, b.ticket));
    bars
}

/// The bars of `lane`'s runs (#84), by start: each from its start to its end, a running one's to
/// `last` (the last event), open.
fn run_bars(runs: &[Run], lane: Lane, last: Timestamp) -> Vec<Bar> {
    runs.iter().filter(|run| run.lane == lane).map(|run| run_bar(run, last)).collect()
}

/// What a run's bar says: the tickets it carried (its id when it carried none), and a failure's
/// cause (`#83 · build/clippy`).
fn run_label(run: &Run) -> String {
    let what = if run.tickets.is_empty() { run.sub.clone() } else { run.tickets.iter().map(|n| format!("#{n}")).collect::<Vec<_>>().join(" ") };
    match run.result {
        GateResult::Failed if run.cause.is_some() || run.step.is_some() => format!("{what} · {}", failure_label(run.cause.as_deref(), run.step.as_deref())),
        GateResult::Failed => format!("{what} · failed"),
        GateResult::Passed | GateResult::NoResult | GateResult::Running => what,
    }
}

fn run_bar(run: &Run, last: Timestamp) -> Bar {
    let end = run.end.unwrap_or(last).max(run.start);
    let style = match run.lane {
        Lane::Gate => Style::GateRun,
        Lane::Ready => Style::ReadyCheck,
    };
    let pr = match run.prs.as_slice() {
        [pr] => Some(*pr),
        _ => None,
    };
    let info = RunBar { sub: run.sub.clone(), lane: run.lane, result: run.result, tickets: run.tickets.clone(), subject: run.subject.clone() };
    Bar {
        pr,
        open: run.end.is_none(),
        colour: run.colour,
        run: Some(info),
        ..Bar::new(run.start, end, run_label(run), None, style, run.tooltip.clone())
    }
}

fn main_bars(merges: &[MainMerge], tickets: &[Ticket], issues: &Issues) -> Vec<Bar> {
    let mut bars: Vec<Bar> = merges.iter().map(|m| main_bar(m, tickets, issues)).collect();
    bars.sort_by_key(|b| b.start);
    bars
}

fn main_bar(merge: &MainMerge, tickets: &[Ticket], issues: &Issues) -> Bar {
    let short = merge.commit.get(..8).unwrap_or(&merge.commit);
    let label = merge.pr.map_or_else(|| short.to_owned(), |pr| format!("PR #{pr}"));
    let titled = merge.pr.and_then(|pr| titles::title(issues, pr).map(|title| format!("PR #{pr} {}", titles::short(title, titles::TOOLTIP_TITLE))));
    let mut tooltip: Vec<TipLine> = titled.map(TipLine::strong).into_iter().collect();
    let subject = if tooltip.is_empty() { TipLine::strong(merge.subject.clone()) } else { TipLine::plain(merge.subject.clone()) };
    tooltip.extend([subject, TipLine::plain(format!("{short} · {}", merge.t))]);
    if let Some(pr) = merge.pr {
        let closed: Vec<String> = tickets.iter().filter(|t| t.merged.as_ref().is_some_and(|m| m.pr == Some(pr))).map(|t| format!("#{}", t.issue)).collect();
        tooltip.push(if closed.is_empty() {
            TipLine::plain("no ticket's merge names it")
        } else {
            TipLine::coloured("merged", PASSED).then(format!(" {}", closed.join(", ")))
        });
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
        assert_eq!(
            shape(&rows),
            ["Merges to main", "Tickets merged", "Gate runs", "Ready checks", "Tickets 1", "Tickets 2"],
            "#168 runs on past #170, so they can't share"
        );
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
        let Node::Ranges { bars, key, .. } = &rows[5] else { panic!() };
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
        let Node::Ranges { bars, .. } = &rows[5] else { panic!() };
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
        // Whatever the spec, a ticket's own bars carry its title: there is no option (#34).
        for spec in [RowsSpec { grouping: Grouping::Workers, collapse: Collapse::Empty }, RowsSpec { grouping: Grouping::Flat, collapse: Collapse::Empty }] {
            let rows = rows_with(&activity, &merges(), spec, &issues);
            let labels: Vec<&str> = rows
                .iter()
                .skip(TOP_KEYS.len())
                .flat_map(Node::leaves)
                .filter_map(|l| if let Node::Ranges { bars, .. } = l { Some(bars) } else { None })
                .flatten()
                .filter(|b| b.ticket == Some(170))
                .map(|b| b.label.as_str())
                .collect();
            assert!(!labels.is_empty() && labels.iter().all(|l| *l == "#170 The spec, written down"), "{spec:?}: {labels:?}");
        }
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
    fn rows_spec_reads_back_and_has_no_titles_option() {
        let spec: RowsSpec = serde_json::from_str(r#"{"grouping":"workers","collapse":"window"}"#).unwrap();
        assert_eq!(spec, RowsSpec { grouping: Grouping::Workers, collapse: Collapse::Window });
        assert_eq!(serde_json::from_str::<RowsSpec>("{}").unwrap(), RowsSpec::default());
        assert_eq!(serde_json::to_string(&RowsSpec::default()).unwrap(), r#"{"grouping":"flat","collapse":"off"}"#, "no titles field (#34)");
        assert_eq!(serde_json::to_string(&spec).unwrap(), r#"{"grouping":"workers","collapse":"window"}"#);
    }

    #[test]
    fn collapse_reads_its_names_and_the_bool_it_was() {
        let read = |text: &str| serde_json::from_str::<RowsSpec>(text).map(|spec| spec.collapse).map_err(|e| e.to_string());
        assert_eq!(read(r#"{"collapse":true}"#), Ok(Collapse::Empty), "before #74, true left out the empty rows");
        assert_eq!(read(r#"{"collapse":false}"#), Ok(Collapse::Off));
        for (name, mode) in [("off", Collapse::Off), ("empty", Collapse::Empty), ("window", Collapse::Window)] {
            assert_eq!(read(&format!(r#"{{"collapse":"{name}"}}"#)), Ok(mode));
            assert_eq!(serde_json::to_value(mode).unwrap(), serde_json::json!(name));
        }
        assert!(read(r#"{"collapse":"sometimes"}"#).is_err());
        assert!(read(r#"{"collapse":1}"#).is_err());
        let schema = serde_json::to_string(&schemars::schema_for!(RowsSpec)).unwrap();
        assert!(schema.contains("window") && schema.contains("boolean"), "{schema}");
        assert!(!Collapse::Off.is_on() && Collapse::Empty.is_on() && Collapse::Window.is_on());
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
    fn the_groups_holding_rows_outermost_first() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec { grouping: Grouping::Workers, ..RowsSpec::default() });
        assert_eq!(groups_holding(&rows, &["w/mini/worker-2/1"]), ["w/mini", "w/mini/worker-2"]);
        assert_eq!(groups_holding(&rows, &["w/mini/worker-1/1", "w/mini/worker-2/1", MAIN_KEY]), ["w/mini", "w/mini/worker-1", "w/mini/worker-2"]);
        assert!(groups_holding(&rows, &[MAIN_KEY, MERGED_KEY]).is_empty(), "the top rows are in no group");
        assert!(groups_holding(&rows, &["w/mini"]).is_empty(), "a group is no row of bars");
        let flat = super::rows(&activity, &[], RowsSpec::default());
        assert!(groups_holding(&flat, &["tickets/1"]).is_empty());
    }

    #[test]
    fn grouped_by_worker() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec { grouping: Grouping::Workers, ..RowsSpec::default() });
        assert_eq!(
            shape(&rows),
            ["Merges to main", "Tickets merged", "Gate runs", "Ready checks", "mini", "  worker-1", "    Tickets 1", "  worker-2", "    Tickets 1"]
        );
        let keys: Vec<&str> = rows.iter().flat_map(Node::leaves).map(Node::key).collect();
        assert_eq!(keys, ["main", "merged", "gates", "ready", "w/mini/worker-1/1", "w/mini/worker-2/1"]);
    }

    #[test]
    fn collapse_leaves_out_empty_rows() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec { collapse: Collapse::Empty, ..RowsSpec::default() });
        assert_eq!(shape(&rows), ["Tickets merged", "Gate runs", "Ready checks", "Tickets 1", "Tickets 2"], "no merges onto main read");
        let window = super::rows(&activity, &[], RowsSpec { collapse: Collapse::Window, ..RowsSpec::default() });
        assert_eq!(window, rows, "to the window, the rows are those with events: the window leaves out more of them");
    }

    /// 01:mm on the day `worker`'s events are.
    fn at(minutes: i64) -> Timestamp {
        Timestamp(Timestamp::parse("2026-10-31T01:00:00Z").unwrap().0 + minutes * 60)
    }

    #[test]
    fn a_window_leaves_out_the_rows_with_nothing_in_it() {
        // tickets/1 holds #1 (0-10) then #3 (10-30); tickets/2 #2 (5-20) then #4 (20-25);
        // tickets/3 #5 (12-15).
        let files = [worker("m-1", "mini", "worker-1", &[(1, 0, 10), (2, 5, 20), (3, 10, 30), (4, 20, 25), (5, 12, 15)])];
        let rows = rows(&Activity::of(&files), &[], RowsSpec { collapse: Collapse::Window, ..RowsSpec::default() });
        let keys = |nodes: &[Node]| nodes.iter().flat_map(Node::leaves).map(|n| n.key().to_owned()).collect::<Vec<_>>();
        assert_eq!(keys(&rows), ["tickets/1", "tickets/2", "tickets/3"], "no merges: the top rows are empty");
        assert_eq!(left_out(&rows, at(0), at(60), &[]), Vec::<String>::new(), "the whole run shows every row");
        assert_eq!(left_out(&rows, at(26), at(40), &[]), ["tickets/2", "tickets/3"], "only #3 runs on past 25");
        assert_eq!(keys(&without(&rows, &left_out(&rows, at(26), at(40), &[]))), ["tickets/1"]);
        assert_eq!(left_out(&rows, at(16), at(19), &[]), ["tickets/3"], "#5 ended at 15");
        assert_eq!(left_out(&rows, at(15), at(19), &[]), Vec::<String>::new(), "a bar ending at the window's start is in it");
        assert_eq!(left_out(&rows, at(26), at(40), &["tickets/3"]), ["tickets/2"], "a row kept (it holds the selection) stays");
        assert_eq!(left_out(&rows, at(31), at(40), &[]), ["tickets/1", "tickets/2", "tickets/3"], "after the last event, nothing");
    }

    #[test]
    fn by_worker_a_group_with_nothing_in_the_window_folds_away_whole() {
        let files = [worker("m-1", "mini", "worker-1", &[(1, 0, 10), (2, 5, 20)]), worker("m-2", "mini", "worker-2", &[(3, 30, 40)])];
        let rows = rows(&Activity::of(&files), &[], RowsSpec { grouping: Grouping::Workers, collapse: Collapse::Window });
        assert_eq!(shape(&rows), ["mini", "  worker-1", "    Tickets 1", "    Tickets 2", "  worker-2", "    Tickets 1"]);
        let out = left_out(&rows, at(12), at(25), &[]);
        assert_eq!(out, ["w/mini/worker-1/1", "w/mini/worker-2"], "worker-2's group goes whole, not row by row");
        assert_eq!(shape(&without(&rows, &out)), ["mini", "  worker-1", "    Tickets 2"]);
        let out = left_out(&rows, at(50), at(60), &[]);
        assert_eq!(out, ["w/mini"], "the machine's group folds away with its workers");
        assert!(without(&rows, &out).is_empty());
        assert_eq!(left_out(&rows, at(50), at(60), &["w/mini/worker-2/1"]), ["w/mini/worker-1"], "a kept row keeps its groups");
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
                Node::Ranges { key, bars, .. } if !TOP_KEYS.contains(&key.as_str()) => {
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
            if TOP_KEYS.contains(&key.as_str()) {
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
        assert_eq!(shape(&rows)[4..], ["Tickets 1", "Tickets 2", "Tickets 3"]);
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
            shape(&by)[4..],
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

    fn every_bar(rows: &[Node]) -> Vec<&Bar> {
        rows.iter()
            .flat_map(Node::leaves)
            .flat_map(|n| match n {
                Node::Ranges { bars, .. } => bars.iter().collect::<Vec<_>>(),
                Node::Group { .. } => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn a_tooltips_lines_are_its_plain_text_styled() {
        for activity in [Activity::of(&sample()), Activity::of(&crate::tickets::tests::abandoned())] {
            let issues = crate::titles::tests::issues();
            for bar in every_bar(&rows_with(&activity, &merges(), RowsSpec::default(), &issues)) {
                assert_eq!(texts(&bar.tooltip_lines), bar.tooltip, "{}", bar.label);
            }
        }
    }

    #[test]
    fn a_tooltip_colours_the_phases_merged_green_sent_back_red_and_durations_strong() {
        use crate::colours::{CLAIMED, SENT_BACK, WAITING, ticket_colour};
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &merges(), RowsSpec::default());
        let Node::Ranges { bars, .. } = &rows[5] else { panic!() };
        // The title strong; each stretch's phase in its bar's colour, its duration strong.
        for bar in bars {
            let tip = &bar.tooltip_lines;
            assert!(tip[0].is_strong("#170 · PR #173"), "{:?}", tip[0]);
            let phase = &tip[1].spans[0];
            assert_eq!(phase.colour, Some(bar.colour), "{} in its bar's colour", phase.text);
            assert!(tip[1].spans.iter().any(|s| s.strong && s.text.ends_with(" s")), "the duration strong: {:?}", tip[1]);
            assert!(tip[2].is_plain(), "by: {:?}", tip[2]);
        }
        let working = &bars[1].tooltip_lines;
        assert_eq!(working[1].colour_of("working"), Some(ticket_colour(170)));
        assert!(working[1].is_strong("13 min 18 s"));
        // The milestones: each name in the colour of what it begins, merged green, the span strong.
        let milestone = |name: &str| working.iter().find(|l| l.spans.first().is_some_and(|s| s.text == name)).and_then(|l| l.spans[0].colour);
        assert_eq!(milestone("claimed"), Some(CLAIMED));
        assert_eq!(milestone("began"), Some(ticket_colour(170)));
        assert_eq!(milestone("finished"), Some(WAITING));
        assert_eq!(milestone("merged"), Some(PASSED));
        assert!(working.iter().any(|l| l.is_strong("35 min 36 s")), "claim to merge strong");
        // The top rows: a merge onto main names what it merged in green; a ticket's merge says
        // merged in green.
        let (Node::Ranges { bars: main, .. }, Node::Ranges { bars: merged, .. }) = (&rows[0], &rows[1]) else { panic!() };
        assert_eq!(main[0].tooltip_lines[2].colour_of("merged"), Some(PASSED));
        assert!(main[0].tooltip_lines[0].is_strong("Merge pull request #173 from mornycorp/170-spec"));
        assert!(main[1].tooltip_lines[2].is_plain(), "no ticket's merge names it: not a failure");
        assert_eq!(merged[0].tooltip_lines[0].colour_of("merged"), Some(PASSED));
        // Sent back red, its note (the cause) red; abandoned red.
        let activity = Activity::of(&crate::tickets::tests::abandoned());
        let rows = super::rows(&activity, &[], RowsSpec::default());
        let sent = every_bar(&rows).into_iter().find(|b| b.style == Style::SentBack && b.ticket == Some(44)).unwrap();
        assert_eq!(sent.tooltip_lines[1].colour_of("sent back"), Some(SENT_BACK));
        assert_eq!(sent.tooltip_lines[3].colour_of("tests fail"), Some(FAILED));
        let gone = every_bar(&rows).into_iter().find(|b| b.ticket == Some(40) && b.style == Style::Working).unwrap();
        assert_eq!(gone.tooltip_lines[3].colour_of("abandoned"), Some(FAILED));
        assert_eq!(gone.tooltip_lines[3].text(), "abandoned: m-1's session ended 2026-10-31T01:10:00Z");
    }

    #[test]
    fn githubs_header_puts_the_title_strong_and_a_merged_pr_green() {
        let activity = Activity::of(&sample());
        let issues = crate::titles::tests::issues();
        let rows = rows_with(&activity, &merges(), RowsSpec::default(), &issues);
        let Node::Ranges { bars, .. } = &rows[5] else { panic!() };
        let tip = &bars[1].tooltip_lines;
        assert!(tip[0].is_strong("#170 The spec, written down"));
        assert!(tip[1].is_plain());
        assert_eq!(tip[2].colour_of("merged"), Some(PASSED));
        assert_eq!(tip[2].text(), "PR #173 Spec: the first draft (merged)");
        let Node::Ranges { bars: main, .. } = &rows[0] else { panic!() };
        assert!(main[0].tooltip_lines[0].is_strong("PR #173 Spec: the first draft"));
        assert!(main[0].tooltip_lines[1].is_plain(), "the subject under the title, plain");
        let Node::Ranges { bars: merged, .. } = &rows[1] else { panic!() };
        assert_eq!(merged[0].tooltip_lines[3].colour_of("merged"), Some(PASSED));
    }
}
