<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Nothing checks it against Swarm Core. -->
# Owner directions

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-11-03 · **Change:** minor (no Gates band, #128; no Gate runs or Ready checks rows, #117, reversing #84) · **Target:** M1

The owner's standing directions and decisions for Swarmception, dated, newest first in each
section. Every agent reads this with `AGENTS.md`. The owner writes it, or an agent records a
direction the owner just gave, quoting it and linking where it was given (an issue, a PR note).
A direction stands until the owner replaces it; a replaced one moves to **Superseded** with the
date it ended.

Write each as: **the direction**, in a sentence or two; *why*, when the owner gave a reason; the
date and where (`(owner, YYYY-MM-DD, #n)`). The protocol and AGENTS.md cite them as
`(owner, YYYY-MM-DD, #n)`.

## The product

- **A tool for looking at swarm behaviour.** It shows a timeline of a swarm's tracking branch,
  for any repository with one (`--repo <path>`), using swarm-timeline's widget. Rows are workers
  or tickets, grouped as a tree (machine / worker / …) or flat, the owner's choice; a Collapse
  option leaves out empty rows. Font Awesome for the app's and toolbar's icons; a View menu
  listing the app's panes (the timeline, for now). The first deliverable: look at what the swarm
  did and pan around in time to see where things happened. Same stack as the other swarms (Rust,
  egui), with the layers swarm-core suggests. It only ever reads a tracking branch, never writes
  one. (owner, 2026-10-31)

## How we work

- **Tickets, not every event** (owner, 2026-10-31, #1): the timeline shows when each ticket was
  claimed, began working, finished working and merged; a row at the top of every merge onto
  `main`, and under it a row of each ticket's merge onto `main`. It replaces the rows by worker
  (sessions, sub-agents, events).
- **Wall clock time** (owner, 2026-10-31, #2): the timeline can show wall-clock time, and shows it.
- **On a Mac, the menus are in the system menu bar** (owner, 2026-10-31, #3), not inside the
  window.
- **Swarm size:** at most three workers per swarm on the owner's desktop, and only while
  `cargo xtask capacity` shows room (the swarm protocol's default; change it here).
- **Merging:** only the coordinator merges, on the local gate
  ([the swarm protocol](swarm-protocol.md#the-coordinators-loop)).
- **GitHub CI runs once a day**, on `main`, not on every push or PR, as for the other swarms.
  Local gates decide merges; a red daily run is `main` red
  ([swarm.md › What CI runs](swarm.md#what-ci-runs)). (owner, 2026-10-31)

## Decisions

| Date | Decision | Why | Where |
|---|---|---|---|
| 2026-11-03 | No Gates band: "We can remove the 'Gate' dock/window; it isn't useful." Kept: `gates.list`, `stats.get`, the Factory's figures and Gate list, the details' gate subjects | the owner's | #128 |
| 2026-11-03 | No Gate runs or Ready checks rows on the timeline (reverses #84's): a gate run that merges sits at the end of its tickets and is already their *in the gate* stretch; a failure is already a send-back on its tickets. Kept: the Gates band's figures, the Factory's Gate list (a click tells a run's story), `gates.list`, the details' gate subjects | the owner's | #117 |
| 2026-11-03 | A stage is drawn and named in one colour everywhere: its strip's (`Stage::colour`), in the tooltip, the widget's segment line and the details' story | the owner's | #117 |
| 2026-10-31 | The swarm runs on swarm-core's protocol and commands | one process shared with the other swarms | the first commit |
| 2026-10-31 | The timeline is swarm-timeline's widget, pinned by revision as a git dependency | one timeline for every swarm tool | the first commit |
| 2026-10-31 | Collapse leaves out rows with no events at all, not rows empty in the window | the rows stay put while panning; a window-relative collapse is G6, built as a third choice, `window` (#74), the default unchanged | the first commit |

## Superseded

None yet.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-03 | minor | No Gates band (#128); no Gate runs or Ready checks rows (#117, reversing #84); a stage in one colour everywhere (#117) |
| 2026-11-02 | minor | The Collapse decision notes that a window-relative choice now exists (#74); the default stays |
| 2026-10-31 | minor | CI once a day on `main` (owner) |
| 2026-10-31 | major | First version, from swarm-core's template |
