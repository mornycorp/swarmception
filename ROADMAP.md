<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Swarmception roadmap

> **Last reviewed:** 2026-11-02 · **Last updated:** 2026-11-02 · **Change:** minor (batch 4: the control channel and MCP, the Details pane, the widget's highlight and durations) · **Target:** M2

One page: the stage, the headline numbers and what's next. Detail lives in
[docs/gaps.md](docs/gaps.md), milestones in [docs/roadmap.md](docs/roadmap.md)
([docs/progress-docs.md](docs/progress-docs.md) rule 8).

**Stage:** M1 built (the tickets' timeline, packed rows, clock time, the macOS menu bar; batch 1), with the owner to pass it. Batch 2 added issue titles and links, a colour per ticket, and early M3 work: the Factory pane and statistics. Batch 3 revised the Factory from the owner's review (the wall, stepping by moment), took colour scheme B, cut the timeline's toolbar to what Swarmception needs, and ended abandoned tickets and gates. Batch 4 built most of M2: a Details pane that tells the story of what is selected, and the control channel and MCP so an agent can drive the app; the widget gained a highlight apart from the selection and durations in hours and minutes. Keeping the state across runs (G5) is the last of M2.

## Headline numbers

Each cell is the number and its source (a test, a command, a generated doc) and links the gap or
doc that holds the detail. Measured beats estimated: say which.

| What | Number | Source |
|---|---|---|
| Scorecard rows done | 8 of 8 (M1); 2 of 4 (M2); 3 of 5 (M3, early), measured | [docs/scorecard.md](docs/scorecard.md) |
| Gaps open | 8 (G9 partial) | [docs/gaps.md](docs/gaps.md) |
| Tickets landed per active hour | mc-engine 7.2 (98 in 13.69 h), swarm-timeline 10.6 (86 in 8.12 h), measured 2026-11-02 (`stats.get`) | [G9](docs/gaps.md#g9-swarm-analytics-panes) |
| Tests | 205, measured 2026-11-02 (`cargo nextest run --workspace`) | [docs/scorecard.md](docs/scorecard.md) |
| Rows for swarm-timeline's 84 tickets | 12 flat, measured 2026-10-31 (82 before #5) | [docs/scorecard.md](docs/scorecard.md) |

## What's next

The same ranked list as [docs/roadmap.md › Current focus](docs/roadmap.md#current-focus).

1. **The state survives a restart** ([G5](docs/gaps.md#g5-the-state-survives-a-restart)): the last of M2
2. **Watching a live swarm** ([G4](docs/gaps.md#g4-watching-a-live-swarm))
3. **The details pane and the timeline select together** ([G16](docs/gaps.md#g16-the-details-pane-and-the-timeline-select-together))

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-02 | minor | Batch 4: stage, scorecard rows (M2), gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 3: stage, scorecard rows, gaps open, landed per active hour, tests; focus moved |
| 2026-11-01 | minor | Batch 2: stage, gaps open, landed per active hour, tests; focus moved |
| 2026-10-31 | minor | Batch 1: stage, gaps open, packed rows; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
