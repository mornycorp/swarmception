<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Swarmception roadmap

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** minor (batch 5: the state kept, live watching, the Details highlight) · **Target:** M3

One page: the stage, the headline numbers and what's next. Detail lives in
[docs/gaps.md](docs/gaps.md), milestones in [docs/roadmap.md](docs/roadmap.md)
([docs/progress-docs.md](docs/progress-docs.md) rule 8).

**Stage:** M1 built (the tickets' timeline, packed rows, clock time, the macOS menu bar; batch 1), with the owner to pass it. Batch 2 added issue titles and links, a colour per ticket, and early M3 work: the Factory pane and statistics. Batch 3 revised the Factory from the owner's review (the wall, stepping by moment), took colour scheme B, cut the timeline's toolbar to what Swarmception needs, and ended abandoned tickets and gates. Batch 4 built most of M2: a Details pane that tells the story of what is selected, and the control channel and MCP so an agent can drive the app; the widget gained a highlight apart from the selection and durations in hours and minutes. Batch 5 finished M2, with the owner to pass it: the state is kept across runs, the app watches a live swarm (a fetch every 30 s, the view following the end), and what the Details pane shows is highlighted on the timeline. M3 (understanding a swarm: analytics panes) is next.

## Headline numbers

Each cell is the number and its source (a test, a command, a generated doc) and links the gap or
doc that holds the detail. Measured beats estimated: say which.

| What | Number | Source |
|---|---|---|
| Scorecard rows done | 8 of 8 (M1); 3 of 4 (M2); 4 of 5 (M3, early), measured | [docs/scorecard.md](docs/scorecard.md) |
| Gaps open | 6 (G9 partial) | [docs/gaps.md](docs/gaps.md) |
| Tickets landed per active hour | mc-engine 7.1 (98 in 13.76 h), swarm-timeline 10.6 (86 in 8.12 h), measured 2026-11-02 (`stats.get`) | [G9](docs/gaps.md#g9-swarm-analytics-panes) |
| Tests | 234, measured 2026-11-02 (`cargo nextest run --workspace`) | [docs/scorecard.md](docs/scorecard.md) |
| Rows for swarm-timeline's 84 tickets | 12 flat, measured 2026-10-31 (82 before #5) | [docs/scorecard.md](docs/scorecard.md) |

## What's next

The same ranked list as [docs/roadmap.md › Current focus](docs/roadmap.md#current-focus).

1. **Automation reaches the window** ([G15](docs/gaps.md#g15-automation-reaches-the-window))
2. **Swarm analytics panes** ([G9](docs/gaps.md#g9-swarm-analytics-panes)): a gates pane first
3. **Issues fetched incrementally** ([G13](docs/gaps.md#g13-issues-fetched-incrementally))

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | Batch 5: stage (M2 built), scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-02 | minor | Batch 4: stage, scorecard rows (M2), gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 3: stage, scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 2: stage, gaps open, landed per active hour, tests; focus moved |
| 2026-10-31 | minor | Batch 1: stage, gaps open, packed rows; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
