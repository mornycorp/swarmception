<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Gaps

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-10-31 · **Change:** minor (batch 1: tickets, packed rows, the clock, the macOS menu) · **Target:** M1

Every known shortfall of Swarmception, ranked: **the work list, and the only home for progress
detail** ([progress-docs.md](progress-docs.md) rule 8). The coordinator folds each batch's merged
PRs' Progress sections into these entries, once per batch, in one commit; workers report in their
PR instead of editing this file. `ROADMAP.md` and [roadmap.md](roadmap.md) link here, one line an
item.

Each entry: its id (`G1`, never reused), what is missing and why it matters, what's done and
what's left, the evidence (code paths, tests, commands, issues), measured numbers with their
source and date, and an estimate in agent-hours (a range). Work an agent can't finish alone is
marked **Human:** with what is needed.

## Ranked

| Rank | Gap | Status | Issues | Estimate |
|---|---|---|---|---|
| 1 | [G10 The macOS View menu holds the system's tab items](#g10-the-macos-view-menu-holds-the-systems-tab-items) | open | #9 | 0.5–1 h |
| 2 | [G2 Automation: the control channel and MCP](#g2-automation-the-control-channel-and-mcp) | open | | 4–8 h |
| 3 | [G3 A details pane for what is selected](#g3-a-details-pane-for-what-is-selected) | open | | 3–6 h |
| 4 | [G11 An abandoned ticket runs on to the last event](#g11-an-abandoned-ticket-runs-on-to-the-last-event) | open | | 1–2 h |
| 5 | [G4 Watching a live swarm](#g4-watching-a-live-swarm) | open | | 2–4 h |
| 6 | [G5 The state survives a restart](#g5-the-state-survives-a-restart) | open | | 2–4 h |
| 7 | [G6 Collapse to the window](#g6-collapse-to-the-window) | open | | 2–4 h |
| 8 | [G7 Tickets carry their titles](#g7-tickets-carry-their-titles) | open | | 2–4 h |
| 9 | [G8 Opening a repository from the app](#g8-opening-a-repository-from-the-app) | open | | 1–2 h |
| 10 | [G9 Swarm analytics panes](#g9-swarm-analytics-panes) | open | | 8–16 h |
| — | [G1 The ruler reads clock time](#g1-the-ruler-reads-clock-time) | done | #2 | |

## G10 The macOS View menu holds the system's tab items

- **Missing:** macOS adds Show Tab Bar and Show All Tabs to a menu titled View; Swarmception has
  one window and no tabs (found in #3, PR #6).
- **Left:** automatic window tabbing off before the menu bar is built (#9).
- **Evidence:** `apps/app/src/mac_menu.rs`.
- **Estimate:** 0.5–1 agent-hours.

## G11 An abandoned ticket runs on to the last event

- **Missing:** a ticket worked with nothing ending it (no stop, release or merge) stays
  *working* to the last event of all: on swarm-timeline, #40 for 90,181 s and #132 for 75,495 s,
  each holding a packed row to the end (measured 2026-10-31, `--repo ../swarm-timeline`).
- **Left:** end a ticket's open stretch when its worker's session ends (or a coordinator's
  `session.stop --of` ends it), marked as abandoned rather than open.
- **Evidence:** `crates/domain/src/tickets.rs`.
- **Estimate:** 1–2 agent-hours.

## G1 The ruler reads clock time

- **Done** (#2, PR #8; swarm-timeline#180): the scrub ruler is a Clock ruler on first load,
  reading UTC (`21:00`, `31 Oct` at midnight, every 3 h at fit); the widget's from/to fields and
  the status bar read `2026-10-30 18:35:57 → 2026-10-31 22:26:46 UTC`; a reload keeps the
  rulers. The Time ruler (seconds from the origin) stays a choice.
- **Left elsewhere:** bars' labels give durations in seconds (`#40 working (90,181 s)`): the
  widget draws them, swarm-timeline#183.
- **Evidence:** `crates/domain/src/timeline.rs` (`clock_epoch`), `crates/ui/src/timeline.rs`,
  `crates/ui/tests/timeline_pane.rs`; screenshot checked 2026-10-31.

## G2 Automation: the control channel and MCP

- **Missing:** the architecture's control channel (`--control`, JSON lines on loopback with a
  token, `ui.inspect`/`ui.set`) and MCP, so an agent can drive the app as a person does.
- **Done:** the engine's commands and the CLI (`commands`, `run`).
- **Left:** a `swarmception-automation` crate (L4) on the pattern of `timeline-automation`;
  `swarmception --control`; `swarmception-cli serve` and `mcp`; forwarding the widget's commands.
- **Evidence:** `apps/cli`, `crates/engine`.
- **Measured:** none yet.
- **Estimate:** 4–8 agent-hours.

## G3 A details pane for what is selected

- **Missing:** a click on a bar or pin shows its tooltip only; the full story (the session's
  events in order, a gate's notes and merges, a ticket's claims and send-backs) needs a pane.
- **Left:** a `rows.details` (or `events.list`) command; a Details pane in the View menu.
- **Estimate:** 3–6 agent-hours.

## G4 Watching a live swarm

- **Missing:** the app reads the branch when asked (Reload, Fetch and Reload). Watching a swarm
  at work wants a fetch on an interval, keeping the view (or following the latest time).
- **Estimate:** 2–4 agent-hours.

## G5 The state survives a restart

- **Missing:** the rows spec, the panes, the window and the widget's saved state
  (`state.save`) start afresh each run.
- **Estimate:** 2–4 agent-hours.

## G6 Collapse to the window

- **Missing:** Collapse leaves out rows with no events at all (owner-directions.md); a mode
  that leaves out rows with nothing in the visible window would follow a pan.
- **Estimate:** 2–4 agent-hours.

## G7 Tickets carry their titles

- **Missing:** a ticket row is named `#170 · PR #173`; its issue's title would say what it is.
  The tracking branch doesn't hold it: it needs `gh` (an L2 source with a deadline, cached).
- **Estimate:** 2–4 agent-hours.

## G8 Opening a repository from the app

- **Missing:** File › Open… (a folder picker) and recent repositories; today `--repo` or the
  current directory.
- **Estimate:** 1–2 agent-hours.

## G9 Swarm analytics panes

- **Missing:** the tracking format's conventions support gate analytics (pass/fail, transient
  causes, send-backs), per-worker throughput and model/effort comparison: panes beside the
  timeline.
- **Estimate:** 8–16 agent-hours, in issues of their own.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-31 | minor | Batch 1: G1 done (#2); G10 (#9) and G11 new; ranking moved |
| 2026-10-31 | major | First version, from swarm-core's template |
