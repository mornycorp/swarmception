<!-- A starting point from swarm-core's templates (`cargo xtask swarm-core init`): this repo's
     own now. Keep it to docs/progress-docs.md's rules; nothing checks it against Swarm Core. -->
# Roadmap

> **Last reviewed:** 2026-10-31 · **Last updated:** 2026-10-31 · **Change:** minor (batch 1: tickets, packed rows, the clock, the macOS menu) · **Target:** M1

Swarmception's milestones, the gates between them and the ranked **Current focus**. Detail lives
in [gaps.md](gaps.md); this page keeps one short sentence a milestone and one line a focus item
([progress-docs.md](progress-docs.md) rule 8). The coordinator edits it, in a batch's progress
commit, only when a milestone or the ranking moves.

## Milestones

| Id | Milestone | Done when |
|---|---|---|
| M1 | Look at what the swarm did | Open any swarm's tracking branch and pan and zoom its tickets' timeline under the merges onto main, packed flat or by worker, in clock time |
| M2 | Read it and drive it | A details pane, the control channel and MCP, the state kept across runs |
| M3 | Understand it | Live watching and analytics panes: gates, send-backs, throughput, models |

## Gates

A milestone is passed when its gaps are closed, with tests and (for UI) a visual check
([progress-docs.md](progress-docs.md) rule 7), and the owner agrees. An architecture review runs
after the first ~20 merges and at each stage gate ([the swarm protocol](swarm-protocol.md#the-coordinators-loop)
step 10).

## Current focus

The same ranked list as `ROADMAP.md`'s **What's next**: its rank, its bold name, its gap link and
at most a short clause. Workers pick from the top ([the swarm protocol](swarm-protocol.md#picking-work)).

1. **No tab items in the macOS View menu** ([G10](gaps.md#g10-the-macos-view-menu-holds-the-systems-tab-items), #9)
2. **Automation: the control channel and MCP** ([G2](gaps.md#g2-automation-the-control-channel-and-mcp))
3. **A details pane** ([G3](gaps.md#g3-a-details-pane-for-what-is-selected))

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-31 | minor | Batch 1: M1 is the tickets' timeline in clock time; the clock left M2; focus moved |
| 2026-10-31 | major | First version, from swarm-core's template |
