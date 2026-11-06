<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Nothing checks it against Swarm Core. -->
# Owner directions

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-11-05 · **Change:** minor (group idle benches, X for row-constrained drag, #147; CI is always local, #140) · **Target:** M1

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
- **CI is always local** (owner, 2026-11-05, #140): no remote actions on GitHub. GitHub Actions
  is turned off for the repository; the coordinator and every worker run `cargo xtask ci` on their
  own machines (workers' `ci --changed`, the coordinator's local gate, the periodic full gate), and
  those decide merges ([swarm.md › What CI runs](swarm.md#what-ci-runs)).

## Decisions

| Date | Decision | Why | Where |
|---|---|---|---|
| 2026-11-06 | Group idle workers into one tile (for example, "3 idle"), with names when they fit and the full list in a tooltip. Ordinary click-drag and Alt-click-drag select the nearest ticket across rows; hold X to constrain either to its starting row. Alt still selects ends. | The owner wants compact benches and the old cross-row default; X adds the row constraint independently of Alt. | Owner, ticket-lifecycle chat; #147 |
| 2026-11-03 | Ticket bars: a sent-back ticket shows the factory's red corner (with the count); merge dots are off by default but can be turned on (the bar ending at its merge says it); a red dot marks a ticket abandoned or cancelled | the owner's | #127 |
| 2026-11-03 | No Gates band: "We can remove the 'Gate' dock/window; it isn't useful." Kept: `gates.list`, `stats.get`, the Factory's figures and Gate list, the details' gate subjects | the owner's | #128 |
| 2026-11-04 | A click or drag on a ticket's bar selects it (in the factory too) with the playhead at its start; with Alt, at its end, where it merged or was given up | the owner's | #137 |
| 2026-11-03 | No Gate runs or Ready checks rows on the timeline (reverses #84's): a gate run that merges sits at the end of its tickets and is already their *in the gate* stretch; a failure is already a send-back on its tickets. Kept: the Gates band's figures, the Factory's Gate list (a click tells a run's story), `gates.list`, the details' gate subjects | the owner's | #117 |
| 2026-11-03 | A stage is drawn and named in one colour everywhere: its strip's (`Stage::colour`), in the tooltip, the widget's segment line and the details' story | the owner's | #117 |
| 2026-10-31 | The swarm runs on swarm-core's protocol and commands | one process shared with the other swarms | the first commit |
| 2026-10-31 | The timeline is swarm-timeline's widget, pinned by revision as a git dependency | one timeline for every swarm tool | the first commit |
| 2026-10-31 | Collapse leaves out rows with no events at all, not rows empty in the window | the rows stay put while panning; a window-relative collapse is G6, built as a third choice, `window` (#74), the default unchanged | the first commit |

## Superseded

- **GitHub CI runs once a day**, on `main`, not on every push or PR (owner, 2026-10-31); ended
  2026-11-05, replaced by **CI is always local** (#140).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-04 | minor | Alt + click on a ticket's bar shows its end (#137) |
| 2026-11-03 | minor | No Gates band (#128); no Gate runs or Ready checks rows (#117, reversing #84); a stage in one colour everywhere (#117) |
| 2026-11-02 | minor | The Collapse decision notes that a window-relative choice now exists (#74); the default stays |
| 2026-10-31 | minor | CI once a day on `main` (owner) |
| 2026-10-31 | major | First version, from swarm-core's template |
