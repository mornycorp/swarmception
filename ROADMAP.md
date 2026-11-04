<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Swarmception roadmap

> **Last reviewed:** 2026-11-04 · **Last updated:** 2026-11-04 · **Change:** minor (batch 13: a sent-back ticket's red corner, merge dots optional, a red dot where given up, no Gates band) · **Target:** M3

One page: the stage, the headline numbers and what's next. Detail lives in
[docs/gaps.md](docs/gaps.md), milestones in [docs/roadmap.md](docs/roadmap.md)
([docs/progress-docs.md](docs/progress-docs.md) rule 8).

**Stage:** M1 built (the tickets' timeline, packed rows, clock time, the macOS menu bar; batch 1), with the owner to pass it. Batch 2 added issue titles and links, a colour per ticket, and early M3 work: the Factory pane and statistics. Batch 3 revised the Factory from the owner's review (the wall, stepping by moment), took colour scheme B, cut the timeline's toolbar to what Swarmception needs, and ended abandoned tickets and gates. Batch 4 built most of M2: a Details pane that tells the story of what is selected, and the control channel and MCP so an agent can drive the app; the widget gained a highlight apart from the selection and durations in hours and minutes. Batch 5 finished M2, with the owner to pass it: the state is kept across runs, the app watches a live swarm (a fetch every 30 s, the view following the end), and what the Details pane shows is highlighted on the timeline. Batch 6 began M3 with a Gates band (every gate run and ready check along the timeline, with pass rates and causes), and finished the automation (MCP through the running window, PNG screenshots, a scripting client) and an incremental issue fetch (0.5 s a reload on mc-engine). Batch 7 finished M3 with a Workers pane (model and effort compared, with sample sizes), made the panes share the window (the timeline keeps its rows; bands fold), and added File ▸ Open and Open Recent. Batch 8 closed the last gaps: Collapse to the window, both Gates lanes per hour, one definition of a gate run, a failed ready check's cause (swarm-core#12), and the Workers window and screenshots over automation. Batch 9 took the owner's review: a ticket's rework after a send-back in its own colour, gate runs and ready checks as timeline rows, the Factory following the timeline (no clock of its own, selection from the timeline), and folding the timeline's quiet time (swarm-timeline#191). Batch 10 tied off the review's loose ends: a row's lanes sized over the visible window (swarm-timeline#197), the minimap marking folds, ready checks in the Factory's Gate list, a send-back's rework logged by swarm-core's `prwatch` (swarm-core#14), tests that wait on the session rather than the clock, and two of the owner's bugs (the Factory's benches with dozens of workers; debug-build markers over the bars). M1, M2 and M3 are built and wait on the owner; Batch 11 drew each ticket in its own colour all along, its stages as a strip along its foot and its send-backs and merge as marks (swarm-timeline#205, #206: the owner's, on mc-engine's #169), kept failures visible when bars pack, read a gate's failure as a send-back, and fixed a test slowed by macOS's first-exec check. Batch 12 made merges zero-length markers, gave a ticket's tooltip its strip's colours, removed the Gate runs and Ready checks rows (the owner reversing batch 9), kept tooltips whole over a short timeline, and stopped tests exec'ing freshly written fakes. Batch 13 took the owner's next look: a sent-back ticket shows the factory's red corner on its bar (swarm-timeline#219), merge dots are optional, a red dot marks a ticket given up, and the Gates band is gone. One small gap is open (G28).

## Headline numbers

Each cell is the number and its source (a test, a command, a generated doc) and links the gap or
doc that holds the detail. Measured beats estimated: say which.

| What | Number | Source |
|---|---|---|
| Scorecard rows done | 8 of 8 (M1); 4 of 4 (M2); 6 of 6 (M3), measured | [docs/scorecard.md](docs/scorecard.md) |
| Gaps open | 1 | [docs/gaps.md](docs/gaps.md) |
| Tickets landed per active hour | mc-engine 5.6 (125 in 22.4 h), swarm-timeline 9.0 (96 in 10.6 h), Swarmception 6.2 (54 in 8.74 h), measured 2026-11-03 (`stats.get`) | [G9](docs/gaps.md#g9-swarm-analytics-panes) |
| Tests | 388, measured 2026-11-03 (`cargo nextest run --workspace`; the Gates band's tests went with it, #128) | [docs/scorecard.md](docs/scorecard.md) |
| Rows for swarm-timeline's 84 tickets | 12 flat, measured 2026-10-31 (82 before #5) | [docs/scorecard.md](docs/scorecard.md) |

## What's next

The same ranked list as [docs/roadmap.md › Current focus](docs/roadmap.md#current-focus).

1. **Small ends after batch 11** ([G28](docs/gaps.md#g28-small-ends-after-batch-11))

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-04 | minor | Batch 13: stage, tests; focus G28 |
| 2026-11-03 | minor | Batch 12: stage, landed per active hour, tests; focus G29, G28 |
| 2026-11-03 | minor | Batch 11: stage, landed per active hour, tests; focus G28 |
| 2026-11-03 | minor | Batch 10: stage, landed per active hour (Swarmception's own too), tests; focus G25 |
| 2026-11-02 | minor | Batch 9: stage, gaps open, landed per active hour, tests; focus G24 |
| 2026-11-02 | minor | Batch 8: stage (no gaps open), gaps open, landed per active hour, tests; focus emptied |
| 2026-11-02 | minor | Batch 7: stage (M3 built), scorecard rows, gaps open, tests; focus moved |
| 2026-11-02 | minor | Batch 6: stage, scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-02 | minor | Batch 5: stage (M2 built), scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-02 | minor | Batch 4: stage, scorecard rows (M2), gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 3: stage, scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 2: stage, gaps open, landed per active hour, tests; focus moved |
| 2026-10-31 | minor | Batch 1: stage, gaps open, packed rows; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
