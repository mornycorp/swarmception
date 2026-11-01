<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Swarmception roadmap

> **Last reviewed:** 2026-11-01 · **Last updated:** 2026-11-01 · **Change:** minor (batch 2: the Factory, statistics, ticket colours, issue titles) · **Target:** M1

One page: the stage, the headline numbers and what's next. Detail lives in
[docs/gaps.md](docs/gaps.md), milestones in [docs/roadmap.md](docs/roadmap.md)
([docs/progress-docs.md](docs/progress-docs.md) rule 8).

**Stage:** M1 built (the tickets' timeline, packed rows, clock time, the macOS menu bar; batch 1), with the owner to pass it. Batch 2 added issue titles and links, a colour per ticket, and early M3 work: the Factory pane and statistics. M2 next.

## Headline numbers

Each cell is the number and its source (a test, a command, a generated doc) and links the gap or
doc that holds the detail. Measured beats estimated: say which.

| What | Number | Source |
|---|---|---|
| Scorecard rows done | 8 of 8 (M1); 2 of 4 (M3, early), measured | [docs/scorecard.md](docs/scorecard.md) |
| Gaps open | 11 (G9 partial) | [docs/gaps.md](docs/gaps.md) |
| Tickets landed per active hour | mc-engine 7.8, swarm-timeline 10.3, measured 2026-11-01 (`stats.get`) | [G9](docs/gaps.md#g9-swarm-analytics-panes) |
| Tests | 130, measured 2026-11-01 (`cargo nextest run --workspace`) | [docs/scorecard.md](docs/scorecard.md) |
| Rows for swarm-timeline's 84 tickets | 12 flat, measured 2026-10-31 (82 before #5) | [docs/scorecard.md](docs/scorecard.md) |

## What's next

The same ranked list as [docs/roadmap.md › Current focus](docs/roadmap.md#current-focus).

1. **No tab items in the macOS View menu** ([G10](docs/gaps.md#g10-the-macos-view-menu-holds-the-systems-tab-items), #9)
2. **An abandoned ticket ends with its worker's session** ([G11](docs/gaps.md#g11-an-abandoned-ticket-runs-on-to-the-last-event))
3. **Automation: the control channel and MCP** ([G2](docs/gaps.md#g2-automation-the-control-channel-and-mcp))

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-01 | minor | Batch 2: stage, gaps open, landed per active hour, tests; focus moved |
| 2026-10-31 | minor | Batch 1: stage, gaps open, packed rows; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
